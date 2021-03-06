use std::sync::mpsc::Receiver;

use crate::blockcfg::{Block, BlockConfig, HasHeader};
use crate::blockchain::BlockchainR;
use crate::intercom::{do_stream_reply, ClientMsg, Error, ReplyStreamHandle};

pub fn client_task<B>(blockchain: BlockchainR<B>, r: Receiver<ClientMsg<B>>)
where
    B: BlockConfig,
    B::Block: HasHeader<Header = B::BlockHeader>,
{
    loop {
        let query = r.recv().unwrap();
        debug!("client query received: {:?}", query);

        match query {
            ClientMsg::GetBlockTip(handler) => handler.reply(handle_get_block_tip(&blockchain)),
            ClientMsg::GetHeaders(ids, handler) => do_stream_reply(handler, |handler| {
                handle_get_headers(&blockchain, ids, handler)
            }),
            ClientMsg::GetHeadersRange(checkpoints, to, handler) => {
                handler.reply(handle_get_headers_range(&blockchain, checkpoints, to))
            }
            ClientMsg::GetBlocks(ids, handler) => do_stream_reply(handler, |handler| {
                handle_get_blocks(&blockchain, ids, handler)
            }),

            ClientMsg::GetBlocksRange(from, to, handler) => do_stream_reply(handler, |handler| {
                handle_get_blocks_range(&blockchain, from, to, handler)
            }),
            ClientMsg::PullBlocksToTip(from, handler) => do_stream_reply(handler, |handler| {
                handle_pull_blocks_to_tip(&blockchain, from, handler)
            }),
        }
    }
}

fn handle_get_block_tip<B>(blockchain: &BlockchainR<B>) -> Result<B::BlockHeader, Error>
where
    B: BlockConfig,
    B::Block: HasHeader<Header = B::BlockHeader>,
{
    let blockchain = blockchain.read().unwrap();
    let tip = blockchain.get_tip();
    let storage = blockchain.storage.read().unwrap();
    match storage.get_block(&tip) {
        Err(err) => Err(Error::failed(format!(
            "Cannot read block '{}': {}",
            tip, err
        ))),
        Ok((blk, _)) => Ok(blk.header()),
    }
}

const MAX_HEADERS: usize = 2000;

fn handle_get_headers_range<B>(
    blockchain: &BlockchainR<B>,
    checkpoints: Vec<B::BlockHash>,
    to: B::BlockHash,
) -> Result<Vec<B::BlockHeader>, Error>
where
    B: BlockConfig,
    B::Block: HasHeader<Header = B::BlockHeader>,
{
    let blockchain = blockchain.read().unwrap();

    /* Filter out the checkpoints that don't exist and sort them by
     * block date. */
    let mut checkpoints = checkpoints
        .iter()
        .filter_map(
            |checkpoint| match blockchain.storage.read().unwrap().get_block(&checkpoint) {
                Err(_) => None,
                Ok((blk, _)) => Some((blk.date(), checkpoint)),
            },
        )
        .collect::<Vec<_>>();

    if !checkpoints.is_empty() {
        /* Start at the newest checkpoint. */
        checkpoints.sort_unstable();
        let from = checkpoints.last().unwrap().1;

        // FIXME: handle checkpoint == genesis

        /* Send headers up to the maximum. */
        let mut headers = vec![];
        for x in blockchain
            .storage
            .read()
            .unwrap()
            .iterate_range(&from, &to)?
        {
            match x {
                Err(err) => return Err(Error::from(err)),
                Ok(info) => {
                    let (block, _) = blockchain
                        .storage
                        .read()
                        .unwrap()
                        .get_block(&info.block_hash)?;
                    headers.push(block.header());
                    if headers.len() >= MAX_HEADERS {
                        break;
                    }
                }
            };
        }

        Ok(headers)
    } else {
        Ok(vec![])
    }
}

fn handle_get_blocks_range<B: BlockConfig>(
    blockchain: &BlockchainR<B>,
    from: B::BlockHash,
    to: B::BlockHash,
    reply: &mut ReplyStreamHandle<B::Block>,
) -> Result<(), Error> {
    let blockchain = blockchain.read().unwrap();

    // FIXME: include the from block
    for x in blockchain
        .storage
        .read()
        .unwrap()
        .iterate_range(&from, &to)?
    {
        let info = x?;
        let (blk, _) = blockchain
            .storage
            .read()
            .unwrap()
            .get_block(&info.block_hash)?;
        reply.send(blk);
    }

    Ok(())
}

fn handle_get_blocks<B: BlockConfig>(
    blockchain: &BlockchainR<B>,
    ids: Vec<B::BlockHash>,
    reply: &mut ReplyStreamHandle<B::Block>,
) -> Result<(), Error> {
    let blockchain = blockchain.read().unwrap();

    for id in ids.into_iter() {
        let (blk, _) = blockchain.storage.read().unwrap().get_block(&id)?;
        reply.send(blk);
    }

    Ok(())
}

fn handle_get_headers<B: BlockConfig>(
    blockchain: &BlockchainR<B>,
    ids: Vec<B::BlockHash>,
    reply: &mut ReplyStreamHandle<B::BlockHeader>,
) -> Result<(), Error> {
    let blockchain = blockchain.read().unwrap();

    for id in ids.into_iter() {
        let (blk, _) = blockchain.storage.read().unwrap().get_block(&id)?;
        reply.send(blk.header());
    }

    Ok(())
}

fn handle_pull_blocks_to_tip<B: BlockConfig>(
    blockchain: &BlockchainR<B>,
    mut from: Vec<B::BlockHash>,
    reply: &mut ReplyStreamHandle<B::Block>,
) -> Result<(), Error> {
    let blockchain = blockchain.read().unwrap();

    // FIXME: handle multiple from addresses
    if from.len() != 1 {
        return Err(Error::unimplemented(
            "only one checkpoint address is currently supported",
        ));
    }
    let from = from.remove(0);

    let tip = blockchain.get_tip();

    for x in blockchain
        .storage
        .read()
        .unwrap()
        .iterate_range(&from, &tip)?
    {
        let info = x?;
        let (blk, _) = blockchain
            .storage
            .read()
            .unwrap()
            .get_block(&info.block_hash)?;
        reply.send(blk);
    }

    Ok(())
}
