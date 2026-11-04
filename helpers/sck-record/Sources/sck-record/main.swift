import Foundation
import SckRecordCore

let helperVersion = "0.1.0"

enum Console {
    private static let lock = NSLock()
    static func out(_ s: String) {
        lock.lock()
        defer { lock.unlock() }
        FileHandle.standardOutput.write((s + "\n").data(using: .utf8)!)
    }
    static func err(_ s: String) {
        lock.lock()
        defer { lock.unlock() }
        FileHandle.standardError.write(("sck-record: " + s + "\n").data(using: .utf8)!)
    }
    static func event(_ e: Event) {
        out(e.json)
    }
}

let command: Command
do {
    command = try ArgumentParser.parse(Array(CommandLine.arguments.dropFirst()))
} catch {
    Console.err("error=usage \(error)")
    Console.err(ArgumentParser.usage)
    exit(HelperExit.usage.rawValue)
}

switch command {
case .help:
    print(ArgumentParser.usage)
    exit(HelperExit.ok.rawValue)
case .version:
    print("sck-record \(helperVersion)")
    exit(HelperExit.ok.rawValue)
case .check:
    guard Permissions.isSupportedOS else {
        print("unsupported-os")
        exit(HelperExit.unsupportedOS.rawValue)
    }
    guard Permissions.screenGranted() else {
        print("permission-denied")
        exit(HelperExit.permissionDenied.rawValue)
    }
    print("ok")
    exit(HelperExit.ok.rawValue)
case .list:
    guard Permissions.isSupportedOS else {
        Console.event(.error(code: "unsupportedOs", detail: Permissions.osVersion))
        exit(HelperExit.unsupportedOS.rawValue)
    }
    Lister.run()
    dispatchMain()
case .snapshot(let options):
    Snapshot.run(options)
    dispatchMain()
case .record(let options):
    let recorder = Recorder(options: options)
    recorder.run()
    dispatchMain()
}
