//! Destination creation belongs to the producer's existing owner transaction.
//! No cell/index mutates before receipt; only bounded capacity is reserved.
use super::*;

pub(super) struct PreparedInsert {
    pub owner: OwnerKey,
    pub value: OwnerData,
    pub encoded: Vec<u8>,
    pub due_tick: u64,
}

impl DurableOwnerStore {
    /// Called last during owner-wave preparation. The outbox was already capped
    /// before capture; deduplicate at most MAX_WAVE_INTENTS addresses, not cells.
    /// A failed preparation acquires no capacity and leaves the wave unchanged.
    pub fn prepare_intent_bootstraps(
        &mut self,
        wave: &mut PreparedOwnerWave,
        outgoing: &[(OwnerKey, bloxgloom_host_api::system::IntentDelivery)],
    ) -> io::Result<()> {
        use super::super::systems::intent::MAX_WAVE_INTENTS;
        if outgoing.len() > MAX_WAVE_INTENTS || !wave.inserts.is_empty() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "invalid bootstrap wave",
            ));
        }
        let mut destinations = BTreeSet::new();
        let mut due_tick = 0;
        for (owner, message) in outgoing {
            due_tick = due_tick.max(
                message
                    .produced_tick
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("bootstrap tick exhausted"))?,
            );
            if self.revision(&wave.system, *owner).is_none() {
                destinations.insert(*owner);
            }
        }
        if destinations.is_empty() {
            return Ok(());
        }
        if self.cells.len() + self.reserved_inserts + destinations.len()
            > super::super::systems::MAX_OWNER_VALUES_PER_SYSTEM
        {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "owner bootstrap capacity full",
            ));
        }
        let descriptor = self.descriptors.get(&wave.system).expect("prepared system");
        let template = descriptor.intent_bootstrap.as_deref().ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                "absent intent destination requires a bootstrap template",
            )
        })?;
        if template.len() > descriptor.max_bytes {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "oversized bootstrap template",
            ));
        }
        let mut inserts = Vec::with_capacity(destinations.len());
        let mut changes = Vec::with_capacity(destinations.len());
        for owner in destinations {
            if !matches!(owner, OwnerKey::Chunk(_)) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "invalid bootstrap owner",
                ));
            }
            let value = descriptor.codec.decode(template).map_err(|_| {
                io::Error::new(ErrorKind::InvalidInput, "invalid bootstrap template")
            })?;
            let encoded = encode_bounded(descriptor, &wave.system, owner, &value)
                .map_err(OwnerDurableError::io)?;
            changes.push(Change::new(
                owner_state_key(&wave.system, owner),
                Vec::new(),
                encode_cell_value(0, descriptor.codec_version, Some(due_tick), &encoded),
            ));
            inserts.push(PreparedInsert {
                owner,
                value,
                encoded,
                due_tick,
            });
        }
        self.reserved_inserts += inserts.len();
        wave.inserts = inserts;
        wave.changes.extend(changes);
        Ok(())
    }

    /// Rejections before WAL admission release only transient capacity. The
    /// normal active/deadline/mailbox indexes still retain the producing work.
    pub fn cancel(&mut self, wave: PreparedOwnerWave) {
        self.reserved_inserts -= wave.inserts.len();
    }
}
