import OpenOpenAppSupport
import SwiftUI

@main
struct OpenOpenApp: App {
  @StateObject private var model = AppModel()

  var body: some Scene {
    Window("OpenOpen", id: "main") {
      OpenOpenRootView(model: model)
    }
    .defaultSize(width: 820, height: 620)

    MenuBarExtra("OpenOpen", systemImage: model.runtimeDisplayState.menuBarSymbol) {
      OpenOpenMenuView(model: model)
    }
  }
}
