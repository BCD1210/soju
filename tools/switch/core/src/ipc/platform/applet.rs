use super::*;
impl Platform {
    pub(super) fn message_event(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let h = ctx
            .kernel
            .register_event(!self.messages.is_empty())
            .map_err(Error::Kernel)?;
        self.message_events.push(h);
        self.owned_events.entry(ctx.object).or_default().push(h);
        Ok(Reply::copy(h))
    }
    pub(super) fn message(&mut self, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        match self.messages.front().copied() {
            None => Ok(Reply::result(0x680)),
            Some(value) => {
                if self.messages.len() == 1 {
                    for &h in &self.message_events {
                        ctx.kernel.clear_event(h).map_err(Error::Kernel)?;
                    }
                }
                self.messages.pop_front();
                Ok(Reply::data(value.to_le_bytes().to_vec()))
            }
        }
    }
    pub(super) fn event(&mut self, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let event = ctx.kernel.register_event(false).map_err(Error::Kernel)?;
        self.owned_events.entry(ctx.object).or_default().push(event);
        Ok(Reply::copy(event))
    }
    pub(super) fn signaled_event(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let event = ctx.kernel.register_event(true).map_err(Error::Kernel)?;
        self.owned_events.entry(ctx.object).or_default().push(event);
        Ok(Reply::copy(event))
    }
    pub(super) fn create_player(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        if req.u32(0)? != 0x10 || req.u32(4)? != 0 {
            return Err(Error::Malformed("unsupported player selector"));
        }
        let h = self.session(Kind::Player, ctx)?;
        self.players.insert(
            ctx.kernel.object_id(h).map_err(Error::Kernel)?,
            Player::default(),
        );
        Ok(Reply::object(h))
    }
    pub(super) fn player_event(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let player = self
            .players
            .get_mut(&ctx.object)
            .ok_or(Error::Malformed("player object"))?;
        let event = match player.event {
            Some(h) => h,
            None => {
                let h = ctx
                    .kernel
                    .register_event(player.completed)
                    .map_err(Error::Kernel)?;
                player.event = Some(h);
                self.owned_events.entry(ctx.object).or_default().push(h);
                h
            }
        };
        Ok(Reply::copy(event))
    }
    pub(super) fn player_push(
        &mut self,
        req: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let [id] = req.objects.as_slice() else {
            return Err(Error::Malformed("player input count"));
        };
        let object = self
            .objects
            .get(id)
            .ok_or(Error::Malformed("player input object"))?;
        if object.kind != Kind::Storage || object.client != ctx.client {
            return Err(Error::Malformed("player input owner/type"));
        }
        let bytes = self
            .storage
            .get(id)
            .ok_or(Error::Malformed("player input storage"))?
            .borrow()
            .clone();
        let player = self
            .players
            .get_mut(&ctx.object)
            .ok_or(Error::Malformed("player object"))?;
        if player.completed || player.inputs.len() >= 2 {
            return Err(Error::Malformed("player input state"));
        }
        player.inputs.push(bytes);
        Ok(Reply::success())
    }
    pub(super) fn player_start(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let player = self
            .players
            .get_mut(&ctx.object)
            .ok_or(Error::Malformed("player object"))?;
        let [common, settings] = player.inputs.as_slice() else {
            return Err(Error::Malformed("player arguments"));
        };
        let event = player
            .event
            .ok_or(Error::Malformed("player event missing"))?;
        if player.completed
            || common.len() != 0x20
            || settings.len() != 0xa0
            || read_u32(common, 0)? != 1
            || read_u32(common, 4)? != 0x20
            || read_u32(common, 8)? != 0x20000
            || read_u32(settings, 0)? != 0
            || settings[0x90] != 0
        {
            return Err(Error::Malformed("player argument version/state"));
        }
        let mut uid = [0; 16];
        uid[0] = 1;
        if settings[8..0x88].chunks_exact(16).any(|u| u == uid) {
            return Err(Error::Malformed("local user excluded"));
        }
        ctx.kernel.signal_event(event).map_err(Error::Kernel)?;
        let mut output = vec![0; 24];
        output[8] = 1;
        player.output = Some(output);
        player.completed = true;
        Ok(Reply::success())
    }
    pub(super) fn player_result(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        if !self
            .players
            .get(&ctx.object)
            .ok_or(Error::Malformed("player object"))?
            .completed
        {
            return Err(Error::Malformed("player incomplete"));
        }
        Ok(Reply::success())
    }
    pub(super) fn player_pop(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        let bytes = self
            .players
            .get(&ctx.object)
            .and_then(|p| p.output.clone())
            .ok_or(Error::Malformed("player output missing"))?;
        let reply = self.storage_object(bytes, ctx)?;
        self.players
            .get_mut(&ctx.object)
            .ok_or(Error::Malformed("player object"))?
            .output = None;
        Ok(reply)
    }
}
