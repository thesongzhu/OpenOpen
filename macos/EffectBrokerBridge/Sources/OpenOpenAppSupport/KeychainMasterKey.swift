import EffectBrokerBridge
import Foundation

enum KeychainMasterKey {
  static func loadOrCreate() throws -> Data {
    try KeychainCoreAuthorityStore().loadOrCreateMasterKey()
  }
}
