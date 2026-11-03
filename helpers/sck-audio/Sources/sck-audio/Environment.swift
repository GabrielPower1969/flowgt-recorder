import CoreGraphics
import Foundation

enum Environment {
    static var osVersion: OperatingSystemVersion { ProcessInfo.processInfo.operatingSystemVersion }

    static var osVersionString: String {
        let v = osVersion
        return "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    /// SCStreamConfiguration.capturesAudio is API_AVAILABLE(macos(13.0)).
    static var isSupportedOS: Bool { osVersion.majorVersion >= 13 }

    /// TCC "Screen Recording" for the *responsible process* (the app that spawned us), without prompting.
    static func screenCaptureGranted() -> Bool { CGPreflightScreenCaptureAccess() }
}

enum Console {
    static func err(_ line: String) {
        FileHandle.standardError.write(Data((line + "\n").utf8))
    }

    static func out(_ line: String) {
        FileHandle.standardOutput.write(Data((line + "\n").utf8))
    }
}
