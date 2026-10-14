// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Function",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Function", targets: ["FunctionApp"]),
        .library(name: "FunctionCore", targets: ["FunctionCore"])
    ],
    targets: [
        .target(name: "FunctionCore"),
        .target(name: "FunctionPlatform", dependencies: ["FunctionCore"]),
        .executableTarget(name: "FunctionApp", dependencies: ["FunctionCore", "FunctionPlatform"],
                          resources: [.process("Resources")]),
        .testTarget(name: "FunctionCoreTests", dependencies: ["FunctionCore", "FunctionPlatform"])
    ],
    swiftLanguageModes: [.v6]
)
