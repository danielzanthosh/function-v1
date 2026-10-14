import Foundation

public enum KeyboardText {
    // CGEvent accepts short UTF-16 payloads. Never split a Unicode scalar's surrogate pair.
    public static func chunks(_ text: String) -> [[UInt16]] {
        var result: [[UInt16]] = []
        var chunk: [UInt16] = []
        for scalar in text.unicodeScalars {
            let units = Array(String(scalar).utf16)
            if chunk.count + units.count > 20 { result.append(chunk); chunk = [] }
            chunk.append(contentsOf: units)
        }
        if !chunk.isEmpty { result.append(chunk) }
        return result
    }
}
