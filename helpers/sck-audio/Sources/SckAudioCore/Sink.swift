import Foundation

public enum WriterError: Error, Equatable {
    /// The reader (ffmpeg) closed its end of the pipe — EPIPE. Treated as a normal end.
    case readerClosed
    case io(errno: Int32, message: String)
    case openFailed(path: String, errno: Int32, message: String)
    case openTimeout(path: String)
    case cancelled
}

/// Where converted PCM goes. Real runs use a file descriptor; tests use memory.
public protocol ByteWriter: AnyObject {
    func write(_ data: Data) throws
    func flushAndClose() throws
}

public final class MemoryWriter: ByteWriter {
    private let lock = NSLock()
    private var storage = Data()
    public private(set) var closed = false

    public init() {}

    public var bytes: Data {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }

    public func write(_ data: Data) throws {
        lock.lock()
        storage.append(data)
        lock.unlock()
    }

    public func flushAndClose() throws {
        lock.lock()
        closed = true
        lock.unlock()
    }
}

/// Blocking writes to a POSIX descriptor with partial-write and EINTR handling.
public final class FileDescriptorWriter: ByteWriter {
    public let fd: Int32
    private let closeOnFinish: Bool
    private var isClosed = false

    public init(fd: Int32, closeOnFinish: Bool = true) {
        self.fd = fd
        self.closeOnFinish = closeOnFinish
    }

    public func write(_ data: Data) throws {
        try data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            guard var pointer = buffer.baseAddress else { return }
            var remaining = buffer.count
            while remaining > 0 {
                let n = Darwin.write(fd, pointer, remaining)
                if n < 0 {
                    let e = errno
                    if e == EINTR { continue }
                    if e == EAGAIN {
                        usleep(1_000)
                        continue
                    }
                    if e == EPIPE { throw WriterError.readerClosed }
                    throw WriterError.io(errno: e, message: String(cString: strerror(e)))
                }
                pointer += n
                remaining -= n
            }
        }
    }

    public func flushAndClose() throws {
        guard closeOnFinish, !isClosed else { return }
        isClosed = true
        close(fd)
    }
}

public enum OutputKind: String {
    case stdout
    case fifo
    case file
}

/// Lazily opens the output path. A FIFO is opened with O_NONBLOCK in a poll loop so the wait for
/// the reader can be cancelled or time out; once connected the descriptor is switched back to
/// blocking mode so back-pressure from ffmpeg works normally.
public enum OutputOpener {
    public static func open(
        path: String,
        timeout: Double?,
        pollInterval: TimeInterval = 0.05,
        isCancelled: () -> Bool = { false }
    ) throws -> (FileDescriptorWriter, OutputKind) {
        if path == "-" {
            return (FileDescriptorWriter(fd: STDOUT_FILENO, closeOnFinish: false), .stdout)
        }

        var st = stat()
        if stat(path, &st) == 0, (st.st_mode & S_IFMT) == S_IFIFO {
            let deadline = timeout.map { Date().addingTimeInterval($0) }
            while true {
                if isCancelled() { throw WriterError.cancelled }
                let fd = Darwin.open(path, O_WRONLY | O_NONBLOCK)
                if fd >= 0 {
                    let flags = fcntl(fd, F_GETFL)
                    if flags >= 0 {
                        _ = fcntl(fd, F_SETFL, flags & ~O_NONBLOCK)
                    }
                    return (FileDescriptorWriter(fd: fd), .fifo)
                }
                let e = errno
                // ENXIO: no reader yet. Anything else is a real failure.
                if e != ENXIO && e != EINTR {
                    throw WriterError.openFailed(path: path, errno: e, message: String(cString: strerror(e)))
                }
                if let deadline, Date() >= deadline {
                    throw WriterError.openTimeout(path: path)
                }
                usleep(useconds_t(max(1, pollInterval * 1_000_000)))
            }
        }

        let fd = Darwin.open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        guard fd >= 0 else {
            let e = errno
            throw WriterError.openFailed(path: path, errno: e, message: String(cString: strerror(e)))
        }
        return (FileDescriptorWriter(fd: fd), .file)
    }
}

/// Serial, bounded, asynchronous writer. The capture callback must never block on ffmpeg, so
/// chunks are queued and written on a private queue; when more than `maxPendingBytes` are
/// waiting (ffmpeg stalled) new chunks are dropped and counted instead of growing memory.
public final class PcmSink {
    public let maxPendingBytes: Int
    private let queue = DispatchQueue(label: "sck-audio.sink")
    private let lock = NSLock()
    private var writer: ByteWriter?
    private var pending = 0
    private var written = 0
    private var dropped = 0
    private var failure: Error?
    private var finished = false

    /// Called once, on the sink's queue, with the first write failure.
    public var onError: ((Error) -> Void)?

    public init(writer: ByteWriter? = nil, maxPendingBytes: Int) {
        self.writer = writer
        self.maxPendingBytes = maxPendingBytes
    }

    public func attach(_ writer: ByteWriter) {
        lock.lock()
        self.writer = writer
        lock.unlock()
    }

    public var bytesWritten: Int { lock.lock(); defer { lock.unlock() }; return written }
    public var droppedBytes: Int { lock.lock(); defer { lock.unlock() }; return dropped }
    public var pendingBytes: Int { lock.lock(); defer { lock.unlock() }; return pending }
    public var error: Error? { lock.lock(); defer { lock.unlock() }; return failure }

    public func enqueue(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        guard failure == nil, !finished, writer != nil else {
            lock.unlock()
            return
        }
        if pending + data.count > maxPendingBytes {
            dropped += data.count
            lock.unlock()
            return
        }
        pending += data.count
        lock.unlock()

        queue.async { [self] in
            lock.lock()
            let target = failure == nil ? writer : nil
            lock.unlock()
            guard let target else {
                lock.lock(); pending -= data.count; lock.unlock()
                return
            }
            do {
                try target.write(data)
                lock.lock()
                pending -= data.count
                written += data.count
                lock.unlock()
            } catch {
                lock.lock()
                pending -= data.count
                let first = failure == nil
                if first { failure = error }
                lock.unlock()
                if first { onError?(error) }
            }
        }
    }

    /// Drains queued writes, closes the writer and reports. Safe to call once.
    public func finish(_ completion: @escaping (Result<Void, Error>) -> Void) {
        lock.lock()
        finished = true
        lock.unlock()
        queue.async { [self] in
            lock.lock()
            let target = writer
            lock.unlock()
            do {
                try target?.flushAndClose()
                completion(.success(()))
            } catch {
                completion(.failure(error))
            }
        }
    }
}
