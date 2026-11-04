use super::*;
impl Platform {
    pub(super) fn storage_object(
        &mut self,
        bytes: Vec<u8>,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let h = self.session(Kind::Storage, ctx)?;
        self.storage.insert(
            ctx.kernel.object_id(h).map_err(Error::Kernel)?,
            Rc::new(RefCell::new(bytes)),
        );
        Ok(Reply::object(h))
    }
    pub(super) fn create_storage(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let size = usize::try_from(req.u64(0)?).map_err(|_| Error::Malformed("storage size"))?;
        if size > 4096 {
            return Err(Error::Malformed("storage size"));
        }
        self.storage_object(vec![0; size], ctx)
    }
    pub(super) fn open_storage(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let bytes = Rc::clone(
            self.storage
                .get(&ctx.object)
                .ok_or(Error::Malformed("storage object"))?,
        );
        let h = self.session(Kind::Accessor, ctx)?;
        self.storage
            .insert(ctx.kernel.object_id(h).map_err(Error::Kernel)?, bytes);
        Ok(Reply::object(h))
    }
    pub(super) fn storage_size(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let size = self
            .storage
            .get(&ctx.object)
            .ok_or(Error::Malformed("storage accessor"))?
            .borrow()
            .len();
        Ok(Reply::data(
            u64::try_from(size)
                .map_err(|_| Error::Malformed("storage size"))?
                .to_le_bytes()
                .to_vec(),
        ))
    }
    pub(super) fn storage_write(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let input = req.input()?;
        let range = storage_range(req, input)?;
        let storage = self
            .storage
            .get(&ctx.object)
            .ok_or(Error::Malformed("storage accessor"))?;
        if range.end > storage.borrow().len() {
            return Err(Error::Malformed("storage range"));
        }
        let bytes = ctx.read_buffer(input, 4096)?;
        storage.borrow_mut()[range].copy_from_slice(&bytes);
        Ok(Reply::success())
    }
    pub(super) fn storage_read(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let output = req.output()?;
        let range = storage_range(req, output)?;
        let storage = self
            .storage
            .get(&ctx.object)
            .ok_or(Error::Malformed("storage accessor"))?
            .borrow();
        let bytes = storage
            .get(range)
            .ok_or(Error::Malformed("storage range"))?
            .to_vec();
        Reply::success().write(output, bytes)
    }
}
fn storage_range(req: &Request, buffer: Buffer) -> Result<std::ops::Range<usize>, Error> {
    let start = usize::try_from(req.u64(0)?).map_err(|_| Error::Malformed("storage offset"))?;
    let len = usize::try_from(buffer.len).map_err(|_| Error::Malformed("storage length"))?;
    Ok(start
        ..start
            .checked_add(len)
            .ok_or(Error::Malformed("storage range"))?)
}
