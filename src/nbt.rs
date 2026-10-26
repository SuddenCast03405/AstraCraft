use std::collections::BTreeMap;

pub const MAX_DEPTH: usize = 64;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COLLECTION: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8), Short(i16), Int(i32), Long(i64), Float(f32), Double(f64),
    ByteArray(Vec<i8>), String(String),
    List { element_type: u8, values: Vec<Tag> },
    Compound(BTreeMap<String, Tag>), IntArray(Vec<i32>), LongArray(Vec<i64>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct NamedTag { pub name: String, pub tag: Tag }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NbtError {
    Truncated, InvalidTag(u8), InvalidUtf8, NegativeLength(i32),
    TooLarge(usize), TooDeep, TrailingBytes(usize), InvalidListType(u8),
}
type Result<T> = std::result::Result<T, NbtError>;

struct Reader<'a> { bytes: &'a [u8], pos: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(NbtError::TooLarge(n))?;
        let s = self.bytes.get(self.pos..end).ok_or(NbtError::Truncated)?;
        self.pos = end; Ok(s)
    }
    fn u8(&mut self) -> Result<u8> { Ok(self.take(1)?[0]) }
    fn i8(&mut self) -> Result<i8> { Ok(self.u8()? as i8) }
    fn i16(&mut self) -> Result<i16> { Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap())) }
    fn u16(&mut self) -> Result<u16> { Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap())) }
    fn i32(&mut self) -> Result<i32> { Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap())) }
    fn i64(&mut self) -> Result<i64> { Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap())) }
    fn f32(&mut self) -> Result<f32> { Ok(f32::from_bits(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))) }
    fn f64(&mut self) -> Result<f64> { Ok(f64::from_bits(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))) }
    fn string(&mut self) -> Result<String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| NbtError::InvalidUtf8)
    }
    fn len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 { return Err(NbtError::NegativeLength(n)); }
        let n = n as usize;
        if n > MAX_COLLECTION { return Err(NbtError::TooLarge(n)); }
        Ok(n)
    }
    fn payload(&mut self, id: u8, depth: usize) -> Result<Tag> {
        if depth > MAX_DEPTH { return Err(NbtError::TooDeep); }
        Ok(match id {
            1 => Tag::Byte(self.i8()?), 2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?), 4 => Tag::Long(self.i64()?),
            5 => Tag::Float(self.f32()?), 6 => Tag::Double(self.f64()?),
            7 => { let n=self.len()?; Tag::ByteArray(self.take(n)?.iter().map(|b| *b as i8).collect()) },
            8 => Tag::String(self.string()?),
            9 => {
                let ty=self.u8()?; if ty > 12 { return Err(NbtError::InvalidListType(ty)); }
                let n=self.len()?; if ty == 0 && n != 0 { return Err(NbtError::InvalidListType(ty)); }
                let mut values=Vec::with_capacity(n.min(4096));
                for _ in 0..n { values.push(self.payload(ty, depth+1)?); }
                Tag::List { element_type:ty, values }
            }
            10 => {
                let mut map=BTreeMap::new();
                loop { let ty=self.u8()?; if ty==0 { break; }
                    if ty>12 { return Err(NbtError::InvalidTag(ty)); }
                    let name=self.string()?; let val=self.payload(ty,depth+1)?; map.insert(name,val);
                }
                Tag::Compound(map)
            }
            11 => { let n=self.len()?; let mut v=Vec::with_capacity(n.min(4096)); for _ in 0..n {v.push(self.i32()?);} Tag::IntArray(v) },
            12 => { let n=self.len()?; let mut v=Vec::with_capacity(n.min(4096)); for _ in 0..n {v.push(self.i64()?);} Tag::LongArray(v) },
            other => return Err(NbtError::InvalidTag(other)),
        })
    }
}
pub fn read_root(bytes: &[u8]) -> Result<NamedTag> {
    if bytes.len() > MAX_BYTES { return Err(NbtError::TooLarge(bytes.len())); }
    let mut r=Reader { bytes, pos:0 };
    let id=r.u8()?; if id==0 || id>12 { return Err(NbtError::InvalidTag(id)); }
    let name=r.string()?; let tag=r.payload(id,0)?;
    if r.pos != bytes.len() { return Err(NbtError::TrailingBytes(bytes.len()-r.pos)); }
    Ok(NamedTag { name, tag })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn int_root() { assert_eq!(read_root(&[3,0,0,0,0,0,42]).unwrap().tag, Tag::Int(42)); }
    #[test] fn empty_compound() { assert_eq!(read_root(&[10,0,0,0]).unwrap().tag, Tag::Compound(BTreeMap::new())); }
    #[test] fn truncated_fails() { assert!(read_root(&[3,0,0]).is_err()); }
}
