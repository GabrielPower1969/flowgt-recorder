import Foundation
import SckAudioCore

let helperVersion = "0.1.0"

let command: Command
do {
    command = try ArgumentParser.parse(Array(CommandLine.arguments.dropFirst()))
} catch {
    Console.err("\(Diag.prefix) error=usage \(error)")
    Console.err(ArgumentParser.usage)
    exit(HelperExit.usage.rawValue)
}

switch command {
case .help:
    Console.out(ArgumentParser.usage)
    exit(HelperExit.ok.rawValue)

case .version:
    Console.out("sck-audio \(helperVersion)")
    exit(HelperExit.ok.rawValue)

case .check:
    guard Environment.isSupportedOS else {
        Console.out("unsupported-os")
        Console.err(Diag.error(.unsupportedOS(Environment.osVersionString)))
        exit(HelperExit.unsupportedOS.rawValue)
    }
    guard Environment.screenCaptureGranted() else {
        Console.out("permission-denied")
        Console.err(Diag.error(.permissionDenied))
        exit(HelperExit.permissionDenied.rawValue)
    }
    Console.out("ok")
    Console.err(Diag.line([("check", ""), ("os", Environment.osVersionString), ("permission", "granted")]))
    exit(HelperExit.ok.rawValue)

case .selfTest(let options):
    let path = NSTemporaryDirectory() + "sck-audio-selftest-\(getpid()).pcm"
    Console.err(Diag.line([
        ("self-test", ""),
        ("duration", Diag.format1(options.durationSeconds ?? CaptureOptions.defaultSelfTestSeconds)),
        ("path", path),
    ]))
    let runner = Runner(options: options, outputPath: path, selfTest: true)
    runner.run()
    dispatchMain()

case .stream(let output, let options):
    let runner = Runner(options: options, outputPath: output, selfTest: false)
    runner.run()
    dispatchMain()
}
