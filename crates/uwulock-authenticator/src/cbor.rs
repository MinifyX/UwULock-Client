//! The little CBOR CTAP2 speaks: integers, byte and text strings, arrays,
//! maps, booleans and null. No floats, no tags, no indefinite lengths — an
//! authenticator may refuse those, and CTAP2's canonical form has none.
//!
//! [`Value::encode`] writes CTAP2's canonical form: the shortest heads, and
//! map keys sorted by their encoded bytes, shorter ones first.

/// How deep a request may nest. CTAP2 itself needs four levels.
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CborError {
    /// Not well-formed, cut short, or more than one value.
    Invalid,
    /// Well-formed, but something CTAP2 never sends: a float, a tag, an
    /// indefinite length, a number beyond 64 bits.
    Unsupported,
}

impl Value {
    pub fn text(text: &str) -> Value {
        Value::Text(text.to_string())
    }

    /// The value under `key` in a map with integer keys, as CTAP2's
    /// commands are.
    pub fn get(&self, key: i64) -> Option<&Value> {
        self.get_key(&Value::Int(key))
    }

    /// The value under a text key, as in WebAuthn's entities.
    pub fn get_text(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find(|(k, _)| matches!(k, Value::Text(t) if t == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    fn get_key(&self, key: &Value) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self, Value::Map(_))
    }

    /// CTAP2's canonical encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        match self {
            Value::Int(n) if *n >= 0 => head(out, 0, *n as u64),
            // -1 - n, without overflow for i64::MIN.
            Value::Int(n) => head(out, 1, !(*n as u64)),
            Value::Bytes(bytes) => {
                head(out, 2, bytes.len() as u64);
                out.extend_from_slice(bytes);
            }
            Value::Text(text) => {
                head(out, 3, text.len() as u64);
                out.extend_from_slice(text.as_bytes());
            }
            Value::Array(items) => {
                head(out, 4, items.len() as u64);
                for item in items {
                    item.encode_into(out);
                }
            }
            Value::Map(entries) => {
                let mut encoded: Vec<(Vec<u8>, Vec<u8>)> = entries
                    .iter()
                    .map(|(k, v)| (k.encode(), v.encode()))
                    .collect();
                encoded.sort_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
                head(out, 5, encoded.len() as u64);
                for (k, v) in encoded {
                    out.extend_from_slice(&k);
                    out.extend_from_slice(&v);
                }
            }
            Value::Bool(false) => out.push(0xf4),
            Value::Bool(true) => out.push(0xf5),
            Value::Null => out.push(0xf6),
        }
    }

    /// Exactly one value, nothing after it.
    pub fn decode(bytes: &[u8]) -> Result<Value, CborError> {
        let mut reader = Reader { bytes, at: 0 };
        let value = reader.value(0)?;
        if reader.at != bytes.len() {
            return Err(CborError::Invalid);
        }
        Ok(value)
    }
}

fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let major = major << 5;
    if n < 24 {
        out.push(major | n as u8);
    } else if n <= 0xff {
        out.extend_from_slice(&[major | 24, n as u8]);
    } else if n <= 0xffff {
        out.push(major | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        out.push(major | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(major | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], CborError> {
        let end = self.at.checked_add(n).ok_or(CborError::Invalid)?;
        let slice = self.bytes.get(self.at..end).ok_or(CborError::Invalid)?;
        self.at = end;
        Ok(slice)
    }

    fn argument(&mut self, info: u8) -> Result<u64, CborError> {
        Ok(match info {
            0..=23 => u64::from(info),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap())),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().unwrap())),
            27 => u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
            31 => return Err(CborError::Unsupported),
            _ => return Err(CborError::Invalid),
        })
    }

    /// A length that has to fit in what is left: a header can't make us
    /// allocate more than the request carries.
    fn length(&mut self, info: u8) -> Result<usize, CborError> {
        let n = self.argument(info)?;
        let left = (self.bytes.len() - self.at) as u64;
        if n > left {
            return Err(CborError::Invalid);
        }
        Ok(n as usize)
    }

    fn value(&mut self, depth: usize) -> Result<Value, CborError> {
        if depth > MAX_DEPTH {
            return Err(CborError::Unsupported);
        }
        let first = self.take(1)?[0];
        let (major, info) = (first >> 5, first & 0x1f);
        Ok(match major {
            0 => {
                Value::Int(i64::try_from(self.argument(info)?).map_err(|_| CborError::Unsupported)?)
            }
            1 => {
                let n = i64::try_from(self.argument(info)?).map_err(|_| CborError::Unsupported)?;
                Value::Int(-1 - n)
            }
            2 => {
                let n = self.length(info)?;
                Value::Bytes(self.take(n)?.to_vec())
            }
            3 => {
                let n = self.length(info)?;
                let text = std::str::from_utf8(self.take(n)?).map_err(|_| CborError::Invalid)?;
                Value::Text(text.to_string())
            }
            4 => {
                let n = self.length(info)?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                Value::Array(items)
            }
            5 => {
                let n = self.length(info)?;
                let mut entries: Vec<(Value, Value)> = Vec::with_capacity(n);
                for _ in 0..n {
                    let key = self.value(depth + 1)?;
                    let value = self.value(depth + 1)?;
                    if entries.iter().any(|(k, _)| *k == key) {
                        return Err(CborError::Invalid);
                    }
                    entries.push((key, value));
                }
                Value::Map(entries)
            }
            6 => return Err(CborError::Unsupported),
            _ => match info {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                22 => Value::Null,
                // undefined, simple values, floats, break
                _ => return Err(CborError::Unsupported),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let value = Value::Map(vec![
            (Value::Int(1), Value::Bytes(vec![1, 2, 3])),
            (Value::Int(-7), Value::text("ES256")),
            (Value::text("up"), Value::Bool(true)),
            (
                Value::Int(3),
                Value::Array(vec![Value::Null, Value::Int(1000)]),
            ),
        ]);
        let encoded = value.encode();
        let decoded = Value::decode(&encoded).unwrap();
        // Canonical order: 1, 3, -7 (0x26), then the text key.
        let Value::Map(entries) = &decoded else {
            panic!()
        };
        let keys: Vec<_> = entries.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(
            keys,
            [
                Value::Int(1),
                Value::Int(3),
                Value::Int(-7),
                Value::text("up")
            ]
        );
        assert_eq!(
            decoded.get(3).unwrap().as_array().unwrap()[1],
            Value::Int(1000)
        );
        assert_eq!(Value::decode(&decoded.encode()).unwrap(), decoded);
    }

    #[test]
    fn known_encodings() {
        assert_eq!(Value::Int(0).encode(), [0x00]);
        assert_eq!(Value::Int(23).encode(), [0x17]);
        assert_eq!(Value::Int(24).encode(), [0x18, 0x18]);
        assert_eq!(Value::Int(-1).encode(), [0x20]);
        assert_eq!(Value::Int(-7).encode(), [0x26]);
        assert_eq!(Value::Int(-257).encode(), [0x39, 0x01, 0x00]);
        assert_eq!(Value::Int(i64::MIN).encode()[0], 0x3b);
        assert_eq!(Value::Int(70000).encode(), [0x1a, 0, 1, 0x11, 0x70]);
        assert_eq!(Value::text("fmt").encode(), [0x63, b'f', b'm', b't']);
        assert_eq!(Value::Bytes(vec![0; 300]).encode()[..3], [0x59, 0x01, 0x2c]);
        assert_eq!(
            Value::decode(&[0x3b, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).unwrap(),
            Value::Int(i64::MIN)
        );
    }

    #[test]
    fn refuses_what_ctap_never_sends() {
        // A float, a tag, an indefinite array, undefined.
        for bytes in [
            &[0xf9, 0x3c, 0x00][..],
            &[0xc0, 0x00],
            &[0x9f, 0xff],
            &[0xf7],
        ] {
            assert_eq!(
                Value::decode(bytes),
                Err(CborError::Unsupported),
                "{bytes:x?}"
            );
        }
        // Cut short, trailing bytes, a length beyond the data, a duplicate
        // key, text that isn't UTF-8, a number beyond i64.
        for bytes in [
            &[0x42, 0x01][..],
            &[0x01, 0x02],
            &[0x5a, 0xff, 0xff, 0xff, 0xff],
            &[0xa2, 0x01, 0x01, 0x01, 0x02],
            &[0x61, 0xff],
            &[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        ] {
            assert!(Value::decode(bytes).is_err(), "{bytes:x?}");
        }
        assert_eq!(
            Value::decode(&[0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(CborError::Unsupported)
        );
        // Nesting deeper than anything CTAP2 sends.
        let deep = [0x81; 20].iter().copied().chain([0x00]).collect::<Vec<_>>();
        assert_eq!(Value::decode(&deep), Err(CborError::Unsupported));
    }
}
