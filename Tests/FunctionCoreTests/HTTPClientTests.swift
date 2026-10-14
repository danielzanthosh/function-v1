import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import Testing
@testable import FunctionCore

final class FixtureProtocol: URLProtocol {
    struct Fixture: Sendable {
        var statuses: [Int]
        let body: Data
        var requests: [URLRequest] = []
    }
    // NSLock works on the macOS 14 deployment target; Synchronization.Mutex requires macOS 15.
    final class FixtureStorage: @unchecked Sendable {
        private let lock = NSLock()
        private var values: [String: Fixture] = [:]
        func withLock<T>(_ operation: (inout [String: Fixture]) -> T) -> T {
            lock.lock(); defer { lock.unlock() }
            return operation(&values)
        }
    }
    static let fixtures = FixtureStorage()
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        guard let url = request.url else { return }
        let result = Self.fixtures.withLock { fixtures -> (Int, Data)? in
            guard var fixture = fixtures[url.absoluteString], !fixture.statuses.isEmpty else { return nil }
            let status = fixture.statuses.removeFirst(); fixture.requests.append(request)
            fixtures[url.absoluteString] = fixture
            return (status, fixture.body)
        }
        guard let (status, body) = result, let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: ["Retry-After": "1"]) else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse)); return
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: body); client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

func fixtureClient(url: String, statuses: [Int], body: String) -> HTTPClient {
    FixtureProtocol.fixtures.withLock { $0[url] = FixtureProtocol.Fixture(statuses: statuses, body: Data(body.utf8)) }
    let config = URLSessionConfiguration.ephemeral; config.protocolClasses = [FixtureProtocol.self]
    return HTTPClient(session: URLSession(configuration: config))
}

@Test func providerSendsAndParsesARealHTTPBoundary() async throws {
    let endpoint = "https://fixture.example/\(UUID().uuidString)/v1"
    let client = fixtureClient(url: endpoint + "/chat/completions", statuses: [200], body: #"{"choices":[{"finish_reason":"tool_calls","message":{"content":null,"tool_calls":[{"id":"id-1","function":{"name":"open_app","arguments":"{\"name\":\"Safari\"}"}}]}}]}"#)
    var config = ProviderConfiguration(); config.baseURL = endpoint
    let reply = try await OpenAIProvider(configuration: config, apiKey: "synthetic", client: client).complete(messages: [ChatMessage(role: .user, content: "Open Safari")], tools: ToolCatalog.definitions())
    #expect(reply.toolCalls.first?.name == "open_app")
    let requests = FixtureProtocol.fixtures.withLock { $0[endpoint + "/chat/completions"]?.requests ?? [] }
    #expect(requests.count == 1)
    #expect(requests[0].httpMethod == "POST")
    #expect(requests[0].value(forHTTPHeaderField: "Authorization") == "Bearer synthetic")
}

@Test func transientProviderErrorsRetryButAuthenticationErrorsDoNot() async throws {
    let retryURL = "https://fixture.example/\(UUID().uuidString)"
    let client = fixtureClient(url: retryURL, statuses: [429, 200], body: "ok")
    #expect(try await client.send(URLRequest(url: URL(string: retryURL)!)) == Data("ok".utf8))
    #expect(FixtureProtocol.fixtures.withLock { $0[retryURL]?.requests.count } == 2)
    let authURL = "https://fixture.example/\(UUID().uuidString)"
    let authClient = fixtureClient(url: authURL, statuses: [401, 200], body: "sensitive body")
    await #expect(throws: FunctionError.self) { try await authClient.send(URLRequest(url: URL(string: authURL)!)) }
    #expect(FixtureProtocol.fixtures.withLock { $0[authURL]?.requests.count } == 1)
}
