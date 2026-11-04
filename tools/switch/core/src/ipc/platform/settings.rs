//! Offline language selection, using libnx set.c command and buffer layouts.
use super::*;
const LANGUAGES: [[u8; 8]; 2] = [*b"ja\0\0\0\0\0\0", *b"en-US\0\0\0"];
pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    tables::c(0, 0, |_, _, _| Ok(Reply::data(LANGUAGES[1].to_vec()))),
    tables::c(1, 0, languages),
    tables::c(2, 4, language),
    tables::c(3, 0, count),
    tables::c(5, 0, languages),
    tables::c(6, 0, count),
];
fn count(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(2u32.to_le_bytes().to_vec()))
}
fn languages(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let buffer = req.output()?;
    let count = buffer.len.min(16) / 8;
    let bytes = LANGUAGES
        .iter()
        .take(usize::try_from(count).map_err(|_| Error::Malformed("language count"))?)
        .flatten()
        .copied()
        .collect();
    Reply::data(
        u32::try_from(count)
            .map_err(|_| Error::Malformed("language count"))?
            .to_le_bytes()
            .to_vec(),
    )
    .write(buffer, bytes)
}
fn language(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let index = usize::try_from(req.u32(0)?).map_err(|_| Error::Malformed("language index"))?;
    let code = LANGUAGES
        .get(index)
        .ok_or(Error::Malformed("unsupported language index"))?;
    Ok(Reply::data(code.to_vec()))
}
