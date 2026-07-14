// swift-tools-version: 6.2

import PackageDescription

let package = Package(
  name: "EffectBrokerBridge",
  platforms: [
    .macOS(.v14)
  ],
  products: [
    .library(name: "EffectBrokerBridge", targets: ["EffectBrokerBridge"]),
    .executable(name: "OpenOpenEffectBroker", targets: ["OpenOpenEffectBroker"]),
  ],
  targets: [
    .target(
      name: "EffectBrokerBridge",
      resources: [
        .copy("Resources/LaunchDaemons")
      ],
      linkerSettings: [
        .linkedFramework("Security"),
        .linkedFramework("ServiceManagement"),
      ]
    ),
    .testTarget(
      name: "EffectBrokerBridgeTests",
      dependencies: ["EffectBrokerBridge"]
    ),
    .executableTarget(
      name: "OpenOpenEffectBroker",
      dependencies: ["EffectBrokerBridge"]
    ),
  ]
)
