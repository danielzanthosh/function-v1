#if os(macOS)
import Foundation
import Security
import FunctionCore

public struct KeychainStore: Sendable {
    private let service = "com.function.assistant.swift"
    public init() {}
    public func read(_ account: String) throws -> String? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service, kSecAttrAccount as String: account,
            kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data, let value = String(data: data, encoding: .utf8) else {
            throw FunctionError.storage("Keychain read failed (\(status)).")
        }
        return value
    }
    public func write(_ value: String, account: String) throws {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service, kSecAttrAccount as String: account]
        if value.isEmpty {
            let status = SecItemDelete(query as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else { throw FunctionError.storage("Keychain delete failed (\(status)).") }
            return
        }
        let attributes: [String: Any] = [kSecValueData as String: Data(value.utf8)]
        let update = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if update == errSecItemNotFound {
            var item = query.merging(attributes) { _, new in new }
            item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            let status = SecItemAdd(item as CFDictionary, nil)
            guard status == errSecSuccess else { throw FunctionError.storage("Keychain save failed (\(status)).") }
        } else if update != errSecSuccess { throw FunctionError.storage("Keychain update failed (\(update)).") }
    }
}
#endif
