import Foundation

enum StrictJSONDocument {
  static func object(from data: Data) -> [String: Any]? {
    var scanner = StrictJSONScanner(bytes: Array(data))
    guard scanner.acceptsSingleValueWithoutDuplicateObjectKeys(),
      let object = try? JSONSerialization.jsonObject(with: data),
      let dictionary = object as? [String: Any]
    else {
      return nil
    }
    return dictionary
  }

  static func canonicalData(from object: [String: Any]) -> Data? {
    try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
  }
}

private struct StrictJSONScanner {
  let bytes: [UInt8]
  var index = 0

  mutating func acceptsSingleValueWithoutDuplicateObjectKeys() -> Bool {
    skipWhitespace()
    guard parseValue(depth: 0) else {
      return false
    }
    skipWhitespace()
    return index == bytes.count
  }

  private mutating func parseValue(depth: Int) -> Bool {
    guard depth <= 64, let byte = peek() else {
      return false
    }
    switch byte {
    case 0x7B:
      return parseObject(depth: depth + 1)
    case 0x5B:
      return parseArray(depth: depth + 1)
    case 0x22:
      return parseStringToken() != nil
    case 0x74:
      return consumeLiteral([0x74, 0x72, 0x75, 0x65])
    case 0x66:
      return consumeLiteral([0x66, 0x61, 0x6C, 0x73, 0x65])
    case 0x6E:
      return consumeLiteral([0x6E, 0x75, 0x6C, 0x6C])
    default:
      return parseNumber()
    }
  }

  private mutating func parseObject(depth: Int) -> Bool {
    guard consume(0x7B) else {
      return false
    }
    skipWhitespace()
    if consume(0x7D) {
      return true
    }

    var keys = Set<String>()
    while true {
      guard let keyToken = parseStringToken(),
        let key = try? JSONDecoder().decode(String.self, from: keyToken),
        keys.insert(key).inserted
      else {
        return false
      }
      skipWhitespace()
      guard consume(0x3A) else {
        return false
      }
      skipWhitespace()
      guard parseValue(depth: depth) else {
        return false
      }
      skipWhitespace()
      if consume(0x7D) {
        return true
      }
      guard consume(0x2C) else {
        return false
      }
      skipWhitespace()
    }
  }

  private mutating func parseArray(depth: Int) -> Bool {
    guard consume(0x5B) else {
      return false
    }
    skipWhitespace()
    if consume(0x5D) {
      return true
    }
    while true {
      guard parseValue(depth: depth) else {
        return false
      }
      skipWhitespace()
      if consume(0x5D) {
        return true
      }
      guard consume(0x2C) else {
        return false
      }
      skipWhitespace()
    }
  }

  private mutating func parseStringToken() -> Data? {
    let start = index
    guard consume(0x22) else {
      return nil
    }
    while let byte = peek() {
      index += 1
      switch byte {
      case 0x22:
        return Data(bytes[start..<index])
      case 0x00...0x1F:
        return nil
      case 0x5C:
        guard let escape = peek() else {
          return nil
        }
        index += 1
        if escape == 0x75 {
          for _ in 0..<4 {
            guard let hex = peek(), isASCIIHex(hex) else {
              return nil
            }
            index += 1
          }
        } else if ![0x22, 0x2F, 0x5C, 0x62, 0x66, 0x6E, 0x72, 0x74].contains(escape) {
          return nil
        }
      default:
        continue
      }
    }
    return nil
  }

  private mutating func parseNumber() -> Bool {
    let start = index
    _ = consume(0x2D)
    if consume(0x30) {
      if let next = peek(), isASCIIDigit(next) {
        return false
      }
    } else {
      guard consumeDigit(in: 0x31...0x39) else {
        return false
      }
      while consumeDigit(in: 0x30...0x39) {}
    }
    if consume(0x2E) {
      guard consumeDigit(in: 0x30...0x39) else {
        return false
      }
      while consumeDigit(in: 0x30...0x39) {}
    }
    if consume(0x65) || consume(0x45) {
      _ = consume(0x2B) || consume(0x2D)
      guard consumeDigit(in: 0x30...0x39) else {
        return false
      }
      while consumeDigit(in: 0x30...0x39) {}
    }
    return index > start
  }

  private mutating func consumeLiteral(_ literal: [UInt8]) -> Bool {
    guard index + literal.count <= bytes.count,
      Array(bytes[index..<(index + literal.count)]) == literal
    else {
      return false
    }
    index += literal.count
    return true
  }

  private mutating func consumeDigit(in range: ClosedRange<UInt8>) -> Bool {
    guard let byte = peek(), range.contains(byte) else {
      return false
    }
    index += 1
    return true
  }

  private mutating func consume(_ byte: UInt8) -> Bool {
    guard peek() == byte else {
      return false
    }
    index += 1
    return true
  }

  private mutating func skipWhitespace() {
    while let byte = peek(), [0x20, 0x09, 0x0A, 0x0D].contains(byte) {
      index += 1
    }
  }

  private func peek() -> UInt8? {
    index < bytes.count ? bytes[index] : nil
  }

  private func isASCIIHex(_ byte: UInt8) -> Bool {
    isASCIIDigit(byte) || (0x41...0x46).contains(byte) || (0x61...0x66).contains(byte)
  }

  private func isASCIIDigit(_ byte: UInt8) -> Bool {
    (0x30...0x39).contains(byte)
  }
}
