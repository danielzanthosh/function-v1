#if os(macOS)
import Foundation
import CryptoKit
import Security
import FunctionCore

public actor ChatGPTAuthentication {
    private struct Tokens: Codable, Sendable {
        var accessToken: String
        var refreshToken: String
        var expiresAt: Date
        var accountID: String?
    }
    public struct Login: Sendable {
        public let url: URL
        fileprivate let verifier: String
        fileprivate let state: String
    }
    private let credentials = KeychainStore()
    private let client = HTTPClient()
    private let clientID = "app_EMoamEEZ73f0CkXaXp7hrann"
    public init() {}
    public func begin() throws -> Login {
        let verifier = try randomString(), state = try randomString()
        let challenge = Self.base64URL(Data(SHA256.hash(data: Data(verifier.utf8))))
        var url = URLComponents(string: "https://auth.openai.com/oauth/authorize")!
        url.queryItems = ["response_type": "code", "client_id": clientID, "redirect_uri": "http://localhost:1455/auth/callback",
            "scope": "openid profile email offline_access", "code_challenge": challenge, "code_challenge_method": "S256",
            "state": state, "id_token_add_organizations": "true", "codex_cli_simplified_flow": "true", "originator": "function"].map { URLQueryItem(name: $0.key, value: $0.value) }
        guard let authorizationURL = url.url else { throw FunctionError.invalidInput("Could not create sign-in URL.") }
        return Login(url: authorizationURL, verifier: verifier, state: state)
    }
    public func finish(_ login: Login, callback: String) async throws {
        guard let components = URLComponents(string: callback), components.scheme == "http", components.host == "localhost",
              components.port == 1455, components.path == "/auth/callback" else { throw FunctionError.invalidInput("Paste the complete localhost callback URL from your browser.") }
        let values = components.queryItems ?? []
        guard values.first(where: { $0.name == "state" })?.value == login.state,
              let code = values.first(where: { $0.name == "code" })?.value else { throw FunctionError.permission("Sign-in state did not match. Start sign-in again.") }
        let tokens = try await exchange(["grant_type": "authorization_code", "code": code, "code_verifier": login.verifier,
                                         "redirect_uri": "http://localhost:1455/auth/callback", "client_id": clientID])
        try save(tokens)
    }
    public func authorization() async throws -> (token: String, accountID: String?) {
        guard let json = try credentials.read("chatgpt_oauth") else { throw FunctionError.permission("Sign in to ChatGPT in Settings first.") }
        var tokens = try JSONDecoder().decode(Tokens.self, from: Data(json.utf8))
        if tokens.expiresAt.timeIntervalSinceNow < 60 {
            tokens = try await exchange(["grant_type": "refresh_token", "refresh_token": tokens.refreshToken, "client_id": clientID])
            try save(tokens)
        }
        return (tokens.accessToken, tokens.accountID)
    }
    public func signOut() throws { try credentials.write("", account: "chatgpt_oauth") }
    private func save(_ tokens: Tokens) throws { try credentials.write(String(decoding: JSONEncoder().encode(tokens), as: UTF8.self), account: "chatgpt_oauth") }
    private func exchange(_ fields: [String: String]) async throws -> Tokens {
        var components = URLComponents(); components.queryItems = fields.map { URLQueryItem(name: $0.key, value: $0.value) }
        let body = Data((components.percentEncodedQuery ?? "").replacingOccurrences(of: "+", with: "%2B").utf8)
        let request = try makeRequest(baseURL: "https://auth.openai.com", path: "oauth/token", apiKey: "", body: body, contentType: "application/x-www-form-urlencoded")
        let response = try JSONDecoder().decode(JSONValue.self, from: await client.send(request))
        let token = try response.requiredString("access_token")
        let expiry = response["expires_in"].number ?? 3600
        guard expiry.isFinite, expiry > 0, expiry < 31_536_000 else { throw FunctionError.provider("Invalid token expiration.") }
        let parts = token.split(separator: ".")
        var account: String?
        if parts.count == 3 {
            var payload = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
            payload += String(repeating: "=", count: (4 - payload.count % 4) % 4)
            if let data = Data(base64Encoded: payload), let claims = try? JSONDecoder().decode(JSONValue.self, from: data) {
                account = claims["https://api.openai.com/auth"]["chatgpt_account_id"].string
            }
        }
        return Tokens(accessToken: token, refreshToken: response["refresh_token"].string ?? fields["refresh_token"] ?? "",
                      expiresAt: Date().addingTimeInterval(expiry), accountID: account)
    }
    private func randomString() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else { throw FunctionError.storage("Could not generate sign-in verifier.") }
        return Self.base64URL(Data(bytes))
    }
    private static func base64URL(_ data: Data) -> String { data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "") }
}
#endif
