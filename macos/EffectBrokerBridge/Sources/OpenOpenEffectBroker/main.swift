import Darwin
import EffectBrokerBridge
import Foundation

guard geteuid() == 0 else {
  fputs("OpenOpenEffectBroker requires the approved root LaunchDaemon context.\n", stderr)
  exit(EXIT_FAILURE)
}

do {
  let backend = try RustBrokerProcessBackend()
  let listener = try BrokerListenerCoordinator(backend: backend)
  listener.activate()
  RunLoop.current.run()
} catch {
  fputs("OpenOpenEffectBroker failed closed during protected startup.\n", stderr)
  exit(EXIT_FAILURE)
}
