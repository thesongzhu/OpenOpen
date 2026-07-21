import AppKit
import OpenOpenAppSupport
import SwiftUI

private final class OpenOpenWindowChromeView: NSView {
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    DispatchQueue.main.async { [weak self] in
      guard let window = self?.window else { return }
      window.titleVisibility = .hidden
      window.titlebarAppearsTransparent = true
      window.titlebarSeparatorStyle = .line
    }
  }
}

private struct OpenOpenWindowChrome: NSViewRepresentable {
  func makeNSView(context: Context) -> OpenOpenWindowChromeView {
    OpenOpenWindowChromeView(frame: .zero)
  }

  func updateNSView(_ view: OpenOpenWindowChromeView, context: Context) {}
}

@main
struct OpenOpenApp: App {
  @StateObject private var model = AppModel()

  var body: some Scene {
    Window("OpenOpen", id: "main") {
      OpenOpenRootView(model: model)
        .background { OpenOpenWindowChrome().frame(width: 0, height: 0) }
    }
    .windowToolbarStyle(.unifiedCompact)
    .defaultSize(width: 820, height: 620)

    MenuBarExtra("OpenOpen", systemImage: model.runtimeDisplayState.menuBarSymbol) {
      OpenOpenMenuView(model: model)
    }
  }
}
