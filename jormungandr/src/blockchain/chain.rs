use crate::{
    blockcfg::{Block, Epoch, Header, HeaderHash, Ledger, Multiverse},
    blockchain::{Branch, Tip, TipGetError, TipReplaceError},
    leadership::{EpochParameters, Leadership, Leaderships},
    start_up::NodeStorage,
    utils::borrow::Borrow,
};
use chain_core::property::{Block as _, HasHeader as _, HasMessages as _, Header as _};
use chain_impl_mockchain::{
    leadership::{self, Verification},
    ledger, multiverse,
};
use chain_storage::error::Error as StorageError;
use chain_storage::store::{self, BlockInfo};
use chain_time::{SlotDuration, TimeFrame, Timeline};
use slog::Logger;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::sync::mpsc;

pub struct Blockchain {
    /// the storage for the overall blockchains (blocks)
    pub storage: Arc<RwLock<NodeStorage>>,

    pub multiverse: Multiverse<Ledger>,

    pub leaderships: Leaderships,

    /// the Tip of the blockchain. This is update as the consensus goes
    pub tip: Tip,

    pub time_frame: TimeFrame,

    pub epoch_event: mpsc::Sender<EpochParameters>,

    /// Incoming blocks whose parent does not exist yet. Sorted by
    /// parent hash to allow quick look up of the children of a
    /// parent.
    ///
    /// FIXME: need some way to GC unconnected blocks after a while.
    pub unconnected_blocks: BTreeMap<HeaderHash, BTreeMap<HeaderHash, Block>>,
}

#[derive(Clone)]
pub struct BlockchainR(Arc<RwLock<Blockchain>>);

impl BlockchainR {
    /// lock the blockchain for read access purpose.
    ///
    /// In the background we are utilising a RwLock. This allows for
    /// multiple Reader to access the blockchain at the same time.
    #[inline]
    pub fn lock_read(&self) -> RwLockReadGuard<Blockchain> {
        match self.0.read() {
            Ok(r) => r,
            Err(err) => panic!("BlockchainR lock is poisoned: {}", err),
        }
    }

    /// lock the blockchain for write access purpose.
    ///
    /// In the background we are utilising a RwLock. This will require
    /// that the multiple reads terminate to acquire the lock for
    /// write purpose (preventing concurrent read)
    #[inline]
    pub fn lock_write(&self) -> RwLockWriteGuard<Blockchain> {
        match self.0.write() {
            Ok(r) => r,
            Err(err) => panic!("BlockchainR lock is poisoned: {}", err),
        }
    }
}

impl From<Blockchain> for BlockchainR {
    fn from(b: Blockchain) -> Self {
        BlockchainR(Arc::new(RwLock::new(b)))
    }
}

// FIXME: copied from cardano-cli
pub const LOCAL_BLOCKCHAIN_TIP_TAG: &'static str = "tip";

custom_error! {pub LoadError
    Storage{source: StorageError} = "Error in the blockchain storage: {source}",
    Ledger{source: ledger::Error} = "Invalid blockchain state: {source}",
    Block0 { source: crate::blockcfg::Block0Error } = "Initial setting of the blockchain are invalid",
}

impl Blockchain {
    pub fn load(
        block_0: Block,
        mut storage: NodeStorage,
        epoch_event: mpsc::Sender<EpochParameters>,
        logger: &Logger,
    ) -> Result<Self, LoadError> {
        use blockcfg::Block0DataSource as _;
        let mut multiverse = multiverse::Multiverse::new();

        let start_time = block_0.start_time()?;
        let slot_duration = block_0.slot_duration()?;

        let time_frame = TimeFrame::new(
            Timeline::new(start_time),
            SlotDuration::from_secs(slot_duration.as_secs() as u32),
        );

        let (tip, leaderships) =
            if let Some(tip_hash) = storage.get_tag(LOCAL_BLOCKCHAIN_TIP_TAG)? {
                info!(logger, "restoring state at tip {}", tip_hash);

                let mut tip = None;

                let block_0_id = block_0.id(); // TODO: get this from the parameter
                let (block_0, _block_0_info) = storage.get_block(&block_0_id)?;
                let mut state = Ledger::new(block_0_id, block_0.messages())?;

                let mut epoch = block_0.date().epoch;
                let initial_leadership = Leadership::new(epoch, &state);
                let mut leaderships = Leaderships::new(&block_0.header, initial_leadership);

                // FIXME: should restore from serialized chain state once we have it.
                info!(logger, "restoring state from block0 {}", block_0_id);
                for info in store::iterate_range(&storage, &block_0_id, &tip_hash)? {
                    let info = info?;
                    let parameters = state.get_ledger_parameters();
                    let block = &storage.get_block(&info.block_hash)?.0;
                    let block_header = &block.header;
                    state = state.apply_block(
                        &parameters,
                        block.messages(),
                        &block.header.to_content_eval_context(),
                    )?;
                    let gc_root = multiverse.add(info.block_hash.clone(), state.clone());
                    if block_header.date().epoch > epoch {
                        epoch = block_header.date().epoch;
                        let leadership = Leadership::new(block_header.date().epoch, &state);
                        let _gc_root = leaderships.add(
                            block_header.date().epoch,
                            block_header.chain_length(),
                            block_header.id(),
                            leadership,
                        );
                    }
                    tip = Some(Tip::new(Branch::new(
                        gc_root,
                        block_header.chain_length(),
                        state.clone(),
                    )));
                }

                (tip.unwrap(), leaderships)
            } else {
                let state = Ledger::new(block_0.id(), block_0.messages())?;
                storage.put_block(&block_0)?;
                let initial_leadership = Leadership::new(block_0.date().epoch, &state);
                let tip = multiverse.add(block_0.id(), state.clone());
                let leaderships = Leaderships::new(&block_0.header, initial_leadership);
                let tip = Tip::new(Branch::new(tip, block_0.header.chain_length(), state));

                (tip, leaderships)
            };

        multiverse.gc();

        Ok(Blockchain {
            storage: Arc::new(RwLock::new(storage)),
            multiverse,
            leaderships,
            tip,
            unconnected_blocks: BTreeMap::default(),
            epoch_event,
            time_frame,
        })
    }

    pub fn initial(&mut self) -> Result<(), StorageError> {
        let (_block, block_info) = self.get_block_tip()?;
        let state = self.get_ledger(&block_info.block_hash).unwrap().clone();
        let slot = self
            .time_frame
            .slot_at(&std::time::SystemTime::now())
            .unwrap();
        let leadership = Leadership::new(0, &state);
        let date = leadership.era().from_slot_to_era(slot).unwrap();

        self.epoch_event
            .try_send(EpochParameters {
                epoch: date.epoch.0,

                ledger_static_parameters: state.get_static_parameters().clone(),
                ledger_parameters: state.get_ledger_parameters(),

                time_frame: self.time_frame.clone(),
                ledger_reference: state,
            })
            .unwrap_or_else(|_| ());
        Ok(())
    }

    pub fn get_ledger(&self, hash: &HeaderHash) -> Option<&Ledger> {
        self.multiverse.get(hash)
    }

    /// return the current tip hash and date
    pub fn get_tip(&self) -> Result<HeaderHash, TipGetError> {
        self.tip.hash()
    }

    pub fn get_block_tip(&self) -> Result<(Block, BlockInfo<HeaderHash>), StorageError> {
        self.get_block(&self.get_tip().unwrap())
    }

    pub fn put_block(&mut self, block: &Block) -> Result<(), StorageError> {
        self.storage.write().unwrap().put_block(block)
    }

    pub fn put_tip(&mut self, branch: Branch, block: &Block) -> Result<(), HandleBlockError> {
        let mut storage = self.storage.write().unwrap();
        storage.put_block(block)?;
        storage.put_tag(LOCAL_BLOCKCHAIN_TIP_TAG, &block.id())?;
        self.tip.replace_with(branch)?;
        Ok(())
    }

    pub fn get_block(
        &self,
        hash: &HeaderHash,
    ) -> Result<(Block, BlockInfo<HeaderHash>), StorageError> {
        self.storage.read().unwrap().get_block(hash)
    }

    fn block_exists(&self, block_hash: &HeaderHash) -> Result<bool, StorageError> {
        // TODO: we assume as an invariant that if a block exists on
        // disk, its ancestors exist on disk as well. Need to make
        // sure that this invariant is preserved everywhere
        // (e.g. loose block GC should delete blocks in reverse
        // order).
        self.storage.read().unwrap().block_exists(block_hash)
    }

    /// get the leadership for the given epoch or build a new one
    /// from the state associated to the given parent_hash
    ///
    /// This function returns None if the `get_ledger(parent_hash)`
    /// call returns None:
    ///
    /// 1. there is no existing leadership for the given epoch;
    /// 2. there is no existing ledger state available for the
    ///    given block
    pub fn get_leadership_or_build<'a>(
        &'a self,
        epoch: Epoch,
        parent_hash: &HeaderHash,
    ) -> Option<Borrow<'a, Leadership>> {
        self.get_leadership(epoch)
            .or_else(|| self.build_leadership(epoch, parent_hash).map(Borrow::Owned))
    }

    pub fn build_leadership(&self, epoch: Epoch, parent_hash: &HeaderHash) -> Option<Leadership> {
        self.get_ledger(parent_hash)
            .map(|ledger| Leadership::new(epoch, ledger))
    }

    pub fn get_leadership<'a>(&'a self, epoch: Epoch) -> Option<Borrow<'a, Leadership>> {
        self.leaderships
            .get(epoch)
            .and_then(|mut iter| iter.next())
            .map(|leadership| leadership.1.into())
    }

    /// Retrieves a list of checkpoint block hashes to pass to another
    /// peer for recovery of missing chain blocks. The checkpoints
    /// traverse back in the history with exponentially receding offsets.
    pub fn get_checkpoints(&self) -> Result<Vec<HeaderHash>, StorageError> {
        let tip = self.get_tip().unwrap();
        let storage = self.storage.read().unwrap();
        let tip_info = storage.get_block_info(&tip)?;
        let mut checkpoints = Vec::new();
        assert!(tip_info.depth > 0);
        store::for_path_to_nth_ancestor(&*storage, &tip, tip_info.depth - 1, |block_info| {
            checkpoints.push(block_info.block_hash.clone());
        })?;
        Ok(checkpoints)
    }
}

custom_error! {pub HandleBlockError
    Storage{source: StorageError} = "Error in the blockchain storage",
    Ledger{source: ledger::Error} = "Invalid blockchain state",
    InternalTip { source: TipReplaceError } = "Cannot update the blockchain's TIP",
}

pub enum HandledBlock {
    /// the block has been rejected
    Rejected { reason: RejectionReason },

    /// More blocks are needed from the network
    ///
    /// TODO: add the block's id and a list of blocks in history
    ///       that can be used to retrieve a common ancestor
    ///       to start the download range from
    MissingBranchToBlock { to: HeaderHash },

    /// the block as been acquired, disseminate to the connected
    /// network that a block has been processed
    Acquired { header: Header },
}

custom_error! {pub RejectionReason
    AlreadyPresent = "Block already present in the blockchain",
    BeyondStabilityDepth = "the block is beyond the stability depth, we reject it",
    Consensus { error: leadership::Error } = "{error}",
}

#[derive(Debug)]
pub enum BlockHeaderTriage {
    /// mark that a block is of no interest for this blockchain
    NotOfInterest { reason: RejectionReason },
    /// the block or header is not connected on the node's blockchain
    /// we need to store it within our cache and try to see if we
    /// can fetch the remaining block
    MissingParentOrBranch { to: HeaderHash },
    /// process the block to the Ledger State
    ProcessBlockToState,
}

pub fn handle_end_of_epoch_event(
    blockchain: &Blockchain,
    epoch: Epoch,
) -> Result<(), HandleBlockError> {
    let (tip, tip_info) = blockchain.get_block_tip()?;
    let state = blockchain.get_ledger(&tip_info.block_hash).unwrap();

    // TODO: get the ledger state from 2 epochs ago

    blockchain
        .epoch_event
        .clone() // clone it to get mutability
        .try_send(EpochParameters {
            epoch: epoch + 1,

            ledger_static_parameters: state.get_static_parameters().clone(),
            ledger_parameters: state.get_ledger_parameters(),

            time_frame: blockchain.time_frame.clone(),
            ledger_reference: state.clone(),
        })
        .unwrap_or_else(|_| ());
    Ok(())
}

pub fn handle_block(
    blockchain: &mut Blockchain,
    block: Block,
    is_tip_candidate: bool,
) -> Result<HandledBlock, HandleBlockError> {
    match header_triage(blockchain, &block.header(), is_tip_candidate)? {
        BlockHeaderTriage::NotOfInterest { reason } => Ok(HandledBlock::Rejected { reason }),
        BlockHeaderTriage::MissingParentOrBranch { to } => {
            // the block is not directly connected to any block
            // in the node blockchain
            // we need to signal the network more blocks are required

            blockchain
                .unconnected_blocks
                .entry(block.parent_id())
                .or_insert(BTreeMap::new())
                .insert(block.id(), block);
            Ok(HandledBlock::MissingBranchToBlock { to })
        }
        BlockHeaderTriage::ProcessBlockToState => {
            //
            process_block(blockchain, block)
        }
    }
}

fn process_block(
    blockchain: &mut Blockchain,
    block: Block,
) -> Result<HandledBlock, HandleBlockError> {
    let (block_tip, _block_tip_info) = blockchain.get_block(&block.parent_id())?;

    let tip_chain_length = block_tip.chain_length();
    let parent_epoch = block_tip.date().epoch;

    let state = {
        let parent_state = blockchain.get_ledger(&block.parent_id()).unwrap();
        let current_parameters = parent_state.get_ledger_parameters();
        parent_state.apply_block(
            &current_parameters,
            block.messages(),
            &block.header.to_content_eval_context(),
        )?
    };

    if block.header.date().epoch > parent_epoch {
        let leadership = Leadership::new(
            block.header.date().epoch,
            blockchain.get_ledger(&block.parent_id()).unwrap(),
        );
        let _gc_root = blockchain.leaderships.add(
            block.header.date().epoch,
            block.header.chain_length(),
            block_tip.id(),
            leadership,
        );
    }

    // FIXME: currently we store all incoming blocks and
    // corresponding states, but to prevent a DoS, we may
    // want to store only sufficiently long chains.

    let new_chain_length = block.chain_length();

    let branch = Branch::new(
        blockchain.multiverse.add(block.id(), state.clone()),
        new_chain_length,
        state,
    );

    if new_chain_length > tip_chain_length {
        blockchain.put_tip(branch, &block)?;
    } else {
        blockchain.put_block(&block)?;
    }

    Ok(HandledBlock::Acquired {
        header: block.header(),
    })
}

pub fn header_triage(
    blockchain: &Blockchain,
    header: &Header,
    is_tip_candidate: bool,
) -> Result<BlockHeaderTriage, HandleBlockError> {
    let block_id = header.id();
    let parent_id = header.parent_id();
    let block_date = header.date();

    if blockchain.block_exists(&block_id)? {
        return Ok(BlockHeaderTriage::NotOfInterest {
            reason: RejectionReason::AlreadyPresent,
        });
    }

    let (block_tip, _) = blockchain.get_block_tip()?;

    if let Some(leadership) = blockchain.get_leadership_or_build(block_date.epoch, &parent_id) {
        match leadership.verify(header) {
            Verification::Success => {}
            Verification::Failure(err) => {
                return Ok(BlockHeaderTriage::NotOfInterest {
                    reason: RejectionReason::Consensus { error: err },
                });
            }
        }
    } else {
        // Error No leadership found for the epoch
        //
        // That the leadership is missing may not be a problem, we might simply
        // need to try to retrieve it (this could be linked with the missing
        // parent or branch (`BlockHeaderTriage::MissingParentOrBranch`)
        unimplemented!()
    }

    // TODO: this is a wrong check, we need to get something more
    //       dynamic than this dummy comparison
    // hint: it might be worth utilising the Clock to know exactly
    // how many blocks there is between the 2 given dates
    // then to use the stability depth to compare if the block
    // is not too far from the blockchain
    //
    if is_tip_candidate && block_date.epoch < block_tip.date().epoch.checked_sub(2).unwrap_or(0) {
        return Ok(BlockHeaderTriage::NotOfInterest {
            reason: RejectionReason::BeyondStabilityDepth,
        });
    }

    if !blockchain.block_exists(&parent_id)? {
        return Ok(BlockHeaderTriage::MissingParentOrBranch { to: parent_id });
    }

    Ok(BlockHeaderTriage::ProcessBlockToState)
}
