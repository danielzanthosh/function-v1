import Foundation

public enum Calculator {
    public static func evaluate(_ expression: String) -> Double? {
        let text = expression.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count <= 256, text.contains(where: { "+-*/%^()".contains($0) }) else { return nil }
        var parser = Parser(characters: Array(text.filter { !$0.isWhitespace }))
        guard let value = try? parser.expression(), parser.index == parser.characters.count, value.isFinite else { return nil }
        return value
    }
    private struct Parser {
        let characters: [Character]
        var index = 0
        var current: Character? { index < characters.count ? characters[index] : nil }
        mutating func consume(_ value: Character) -> Bool {
            if current == value { index += 1; return true }; return false
        }
        mutating func expression() throws -> Double {
            var value = try term()
            while let op = current, op == "+" || op == "-" { index += 1; let rhs = try term(); value = op == "+" ? value + rhs : value - rhs }
            return value
        }
        mutating func term() throws -> Double {
            var value = try power()
            while let op = current, "*/%".contains(op) {
                index += 1; let rhs = try power()
                if rhs == 0 { throw FunctionError.invalidInput("Division by zero") }
                if op == "*" { value *= rhs } else if op == "/" { value /= rhs } else { value = value.truncatingRemainder(dividingBy: rhs) }
            }
            return value
        }
        mutating func power() throws -> Double {
            if consume("-") { return try -power() }
            if consume("+") { return try power() }
            let value = try atom()
            if consume("^") { return try pow(value, power()) }
            return value
        }
        mutating func atom() throws -> Double {
            if consume("(") {
                let value = try expression()
                guard consume(")") else { throw FunctionError.invalidInput("Missing parenthesis") }
                return value
            }
            let start = index
            while let character = current, character.isNumber || character == "." { index += 1 }
            guard start < index, let value = Double(String(characters[start..<index])) else { throw FunctionError.invalidInput("Invalid number") }
            return value
        }
    }
}
