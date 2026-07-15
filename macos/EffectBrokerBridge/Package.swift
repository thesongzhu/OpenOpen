// swift-tools-version: 6.2

import PackageDescription

let package = Package(
  name: "EffectBrokerBridge",
  platforms: [
    .macOS(.v14)
  ],
  products: [
    .library(name: "EffectBrokerBridge", targets: ["EffectBrokerBridge"]),
    .library(name: "OpenOpenAppSupport", targets: ["OpenOpenAppSupport"]),
    .executable(name: "OpenOpen", targets: ["OpenOpen"]),
    .executable(name: "OpenOpenEffectBroker", targets: ["OpenOpenEffectBroker"]),
  ],
  targets: [
    .target(
      name: "EffectBrokerBridge",
      resources: [
        .copy("Resources/LaunchDaemons")
      ],
      linkerSettings: [
        .linkedLibrary("bsm"),
        .linkedFramework("Security"),
        .linkedFramework("ServiceManagement"),
      ]
    ),
    .testTarget(
      name: "EffectBrokerBridgeTests",
      dependencies: ["EffectBrokerBridge"]
    ),
    .target(
      name: "OpenOpenAppSupport",
      dependencies: ["EffectBrokerBridge"],
      linkerSettings: [
        .linkedFramework("Security"),
        .linkedFramework("ServiceManagement"),
      ]
    ),
    .testTarget(
      name: "OpenOpenAppSupportTests",
      dependencies: ["OpenOpenAppSupport"]
    ),
    .executableTarget(
      name: "OpenOpen",
      dependencies: ["OpenOpenAppSupport"]
    ),
    .executableTarget(
      name: "OpenOpenEffectBroker",
      dependencies: ["EffectBrokerBridge"]
    ),
  ]
)
