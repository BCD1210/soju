use super::*;
use std::io::Read;
const SHARED_SIZE: usize = 0x1100000;
pub(super) struct Fonts {
    bytes: Vec<u8>,
    records: [(u32, u32); 6],
}
impl Fonts {
    pub(super) fn load(home: &Path) -> Result<Option<Self>, Error> {
        match Self::load_available(home) {
            Ok(fonts) => Ok(fonts),
            Err(Error::Io(_) | Error::Malformed(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn load_available(home: &Path) -> Result<Option<Self>, Error> {
        let standard = read_optional(
            Path::new("/System/Library/Fonts/Supplemental/Arial.ttf"),
            8 * 1024 * 1024,
        )?;
        let unicode = read_optional(&home.join("host-unicode-bmp.ttf"), 15 * 1024 * 1024)?;
        match standard {
            Some(bytes) => Self::build(bytes, unicode.unwrap_or_default()).map(Some),
            None => Ok(None),
        }
    }
    fn build(mut bytes: Vec<u8>, unicode: Vec<u8>) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 || unicode.len() > 15 * 1024 * 1024 {
            return Err(Error::Malformed("host font size"));
        }
        let mut records = [(
            0,
            u32::try_from(bytes.len()).map_err(|_| Error::Malformed("font length"))?,
        ); 6];
        if !unicode.is_empty() {
            let offset = (bytes.len() + 0xfff) & !0xfff;
            if offset + unicode.len() > SHARED_SIZE {
                return Err(Error::Malformed("host font image size"));
            }
            records.fill((
                u32::try_from(offset).map_err(|_| Error::Malformed("font offset"))?,
                u32::try_from(unicode.len()).map_err(|_| Error::Malformed("font size"))?,
            ));
            bytes.resize(offset, 0);
            bytes.extend_from_slice(&unicode);
        }
        bytes.resize(SHARED_SIZE, 0);
        Ok(Self { bytes, records })
    }
}
fn read_optional(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, Error> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() <= limit => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(limit + 1)
                .read_to_end(&mut bytes)?;
            if u64::try_from(bytes.len()).map_err(|_| Error::Malformed("host font length"))? > limit
            {
                return Err(Error::Malformed("host font grew beyond limit"));
            }
            Ok(Some(bytes))
        }
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
impl Platform {
    fn font(&self, req: &Request) -> Result<(u32, u32), Error> {
        let fonts = self
            .fonts
            .as_ref()
            .ok_or(Error::Malformed("host font unavailable"))?;
        fonts
            .records
            .get(usize::try_from(req.u32(0)?).map_err(|_| Error::Malformed("font type"))?)
            .copied()
            .ok_or(Error::Malformed("font type unavailable"))
    }
    pub(super) fn font_request(
        &mut self,
        req: &Request,
        _: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        self.font(req)?;
        Ok(Reply::success())
    }
    pub(super) fn font_state(
        &mut self,
        req: &Request,
        _: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        self.font(req)?;
        Ok(Reply::data(1u32.to_le_bytes().to_vec()))
    }
    pub(super) fn font_size(&mut self, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
        Ok(Reply::data(self.font(req)?.1.to_le_bytes().to_vec()))
    }
    pub(super) fn font_offset(
        &mut self,
        req: &Request,
        _: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        Ok(Reply::data(self.font(req)?.0.to_le_bytes().to_vec()))
    }
    pub(super) fn font_shared(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let fonts = self
            .fonts
            .as_ref()
            .ok_or(Error::Malformed("host font unavailable"))?;
        if self.font_handle.is_none() {
            self.font_handle = Some(
                ctx.kernel
                    .register_shared(ctx.memory, &fonts.bytes, 1)
                    .map_err(Error::Kernel)?,
            );
        }
        Ok(Reply::copy(
            self.font_handle.ok_or(Error::Malformed("font handle"))?,
        ))
    }
    pub(super) fn font_list(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let fonts = self
            .fonts
            .as_ref()
            .ok_or(Error::Malformed("host font unavailable"))?;
        if req.count(BufferKind::Receive) != 3 || req.count(BufferKind::Exchange) != 0 {
            return Err(Error::Malformed("font list descriptors"));
        }
        let buffers = [
            req.buffer(BufferKind::Receive, 0)?,
            req.buffer(BufferKind::Receive, 1)?,
            req.buffer(BufferKind::Receive, 2)?,
        ];
        for (i, b) in buffers.iter().enumerate() {
            let end = b
                .address
                .checked_add(24)
                .ok_or(Error::Malformed("font output address"))?;
            if b.len < 24
                || b.address < ctx.tls.saturating_add(256) && ctx.tls < end
                || buffers[..i].iter().any(|other| {
                    b.address < other.address.saturating_add(24) && other.address < end
                })
            {
                return Err(Error::Malformed("overlapping font output"));
            }
        }
        let mut reply = Reply::data(vec![1, 0, 0, 0, 6, 0, 0, 0]);
        let arrays = [
            (0u32..6).collect::<Vec<_>>(),
            fonts.records.iter().map(|r| r.0).collect(),
            fonts.records.iter().map(|r| r.1).collect(),
        ];
        for (buffer, values) in buffers.into_iter().zip(arrays) {
            reply = reply.write(
                buffer,
                values.iter().flat_map(|v| v.to_le_bytes()).collect(),
            )?;
        }
        Ok(reply)
    }
}
#[cfg(test)]
pub(super) fn fixture() -> Fonts {
    Fonts::build(vec![0x55; 17], vec![0xaa; 7]).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_font_image_preserves_bytes_and_latin_fallback() {
        let latin = Fonts::build(vec![3; 17], vec![]).unwrap();
        assert_eq!(latin.records, [(0, 17); 6]);
        assert_eq!(&latin.bytes[..17], &[3; 17]);
        assert!(latin.bytes[17..].iter().all(|b| *b == 0));
        let unicode = Fonts::build(vec![3; 17], vec![4; 7]).unwrap();
        assert_eq!(unicode.records, [(4096, 7); 6]);
        assert_eq!(&unicode.bytes[4096..4103], &[4; 7]);
        assert!(Fonts::build(vec![], vec![]).is_err());
        assert!(Fonts::build(vec![0; 8 * 1024 * 1024], vec![0; 15 * 1024 * 1024]).is_err());
    }
}
