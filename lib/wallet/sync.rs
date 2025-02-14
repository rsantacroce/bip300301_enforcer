//! Wallet synchronization

use std::time::SystemTime;

use async_lock::{MutexGuard, RwLockWriteGuard};
use bdk_esplora::EsploraAsyncExt as _;
use bdk_wallet::{file_store::Store, ChangeSet, FileStoreError};
use either::Either;

use crate::{
    types::WithdrawalBundleEventKind,
    wallet::{
        error,
        util::{RwLockUpgradableReadGuardSome, RwLockWriteGuardSome},
        BdkWallet, WalletInner,
    },
};

/// Write-locked last_sync, wallet, and database
#[must_use]
pub(in crate::wallet) struct SyncWriteGuard<'a> {
    database: MutexGuard<'a, Store<ChangeSet>>,
    last_sync: RwLockWriteGuard<'a, Option<SystemTime>>,
    pub(in crate::wallet) wallet: RwLockWriteGuardSome<'a, BdkWallet>,
}

impl SyncWriteGuard<'_> {
    /// Persist changes from the sync
    pub(in crate::wallet) fn commit(mut self) -> Result<(), FileStoreError> {
        self.wallet
            .with_mut(|wallet| wallet.persist(&mut self.database))?;
        *self.last_sync = Some(SystemTime::now());
        Ok(())
    }
}

impl WalletInner {
    pub(in crate::wallet) async fn handle_connect_block(
        &self,
        block: &bitcoin::Block,
        block_height: u32,
        block_info: crate::types::BlockInfo,
    ) -> Result<(), error::ConnectBlock> {
        // Acquire a wallet lock immediately, so that it does not update
        // while other dbs are being written to
        let mut wallet_write = self.write_wallet().await?;
        let finalized_withdrawal_bundles =
            block_info
                .withdrawal_bundle_events()
                .filter_map(|event| match event.kind {
                    WithdrawalBundleEventKind::Failed
                    | WithdrawalBundleEventKind::Succeeded {
                        sequence_number: _,
                        transaction: _,
                    } => Some((event.sidechain_id, event.m6id)),
                    WithdrawalBundleEventKind::Submitted => None,
                });
        let () = self.delete_bundle_proposals(finalized_withdrawal_bundles)?;
        let sidechain_proposal_ids = block_info
            .sidechain_proposals()
            .map(|(_vout, proposal)| proposal.compute_id());
        let () = self.delete_pending_sidechain_proposals(sidechain_proposal_ids)?;
        let mut database = self.bitcoin_db.lock().await;
        wallet_write.with_mut(|wallet| {
            let () = wallet.apply_block(block, block_height)?;
            wallet
                .persist(&mut database)
                .map_err(error::ConnectBlock::from)
        })?;
        drop(wallet_write);
        Ok(())
    }

    /// Sync the wallet, returning a write guard on last_sync, wallet, and database
    /// if wallet was not locked.
    /// Does not commit changes.
    #[allow(clippy::significant_drop_in_scrutinee, reason = "false positive")]
    pub(in crate::wallet) async fn sync_lock(
        &self,
    ) -> Result<Option<SyncWriteGuard>, error::WalletSync> {
        let start = SystemTime::now();
        tracing::trace!("starting wallet sync");
        // Hold an upgradable lock for the duration of the sync, to prevent other
        // updates to the wallet between fetching an update via the chain source,
        // and applying the update.
        // Don't error out here if the wallet is locked, just skip the sync.
        let wallet_read = {
            match self.read_wallet_upgradable().await {
                Ok(wallet_read) => wallet_read,
                // "Accepted" errors, that aren't really errors in this case.
                Err(error::NotUnlocked) => {
                    tracing::trace!("sync: skipping sync due to wallet error");
                    return Ok(None);
                }
            }
        };
        tracing::trace!("Acquired upgradable read lock on wallet");
        let last_sync_write = self.last_sync.write().await;
        let request = wallet_read.start_sync_with_revealed_spks().build();

        tracing::trace!(
            spks = request.progress().spks_remaining,
            txids = request.progress().txids_remaining,
            outpoints = request.progress().outpoints_remaining,
            "Requesting sync via chain source"
        );
        const PARALLEL_REQUESTS: usize = 5;
        const BATCH_SIZE: usize = 5;
        const FETCH_PREV_TXOUTS: bool = false;
        let (source, update) = match &self.chain_source {
            Either::Left(electrum_client) => (
                "electrum",
                electrum_client.sync(request, BATCH_SIZE, FETCH_PREV_TXOUTS)?,
            ),
            Either::Right(esplora_client) => (
                "esplora",
                esplora_client.sync(request, PARALLEL_REQUESTS).await?,
            ),
        };
        tracing::trace!("Fetched update from {source}, applying update");
        // Upgrade wallet lock
        let mut wallet_write = RwLockUpgradableReadGuardSome::upgrade(wallet_read).await;
        wallet_write.with_mut(|wallet| wallet.apply_update(update))?;
        tracing::debug!(
            "wallet sync complete in {:?}",
            start.elapsed().unwrap_or_default(),
        );
        Ok(Some(SyncWriteGuard {
            database: self.bitcoin_db.lock().await,
            last_sync: last_sync_write,
            wallet: wallet_write,
        }))
    }

    /// Sync the wallet if the wallet is not locked, committing changes
    #[allow(clippy::significant_drop_in_scrutinee, reason = "false positive")]
    pub(in crate::wallet) async fn sync(&self) -> Result<(), error::WalletSync> {
        match self.sync_lock().await? {
            Some(sync_write) => {
                tracing::trace!("obtained sync lock, committing changes");
                let () = sync_write.commit()?;
                Ok(())
            }
            None => {
                tracing::trace!("no sync lock, skipping commit");
                Ok(())
            }
        }
    }
}
