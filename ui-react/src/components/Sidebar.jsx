import React from 'react';

export default function Sidebar({
  peerAddress,
  peerInfo,
  messageCount,
  txidInput,
  setTxidInput,
  handleFetchTx,
  isFetchingTx,
  mempoolStats,
  handleFetchMempool,
  isFetchingMempool
}) {
  return (
    <aside className="sidebar">
      {/* Peer info */}
      <div className="card">
        <div className="card-title">Peer Info</div>
        <div className="info-row"><span className="label">Address</span><span>{peerAddress}</span></div>
        <div className="info-row"><span className="label">Agent</span><span>{peerInfo.agent}</span></div>
        <div className="info-row"><span className="label">Height</span><span>{peerInfo.height}</span></div>
        <div className="info-row"><span className="label">Protocol</span><span>{peerInfo.version}</span></div>
        <div className="info-row"><span className="label">Services</span><span>{peerInfo.services}</span></div>
        <div className="info-row"><span className="label">Messages</span><span>{messageCount}</span></div>
      </div>

      {/* TX Lookup */}
      <div className="card">
        <div className="card-title">Look Up Transaction</div>
        <p className="hint">Works for any transaction — confirmed or unconfirmed — via mempool.space.</p>
        <input
          id="txid-input"
          className="text-input"
          placeholder="Paste 64-char TXID here..."
          maxLength="64"
          value={txidInput}
          onChange={(e) => setTxidInput(e.target.value)}
        />
        <button
          className="btn btn-primary full-width"
          onClick={handleFetchTx}
          disabled={isFetchingTx || txidInput.trim().length !== 64}
        >
          {isFetchingTx ? 'Fetching...' : 'Fetch Transaction'}
        </button>
      </div>

      {/* Mempool */}
      <div className="card">
        <div className="card-title">Mempool Snapshot</div>
        <p className="hint">Fetches all unconfirmed transactions via mempool.space — reliable and instant.</p>
        {mempoolStats && (
          <div className="mempool-stats-row">
            <div className="mempool-stat"><div className="mempool-stat-val">{mempoolStats.count?.toLocaleString()}</div><div className="mempool-stat-lbl">TXs</div></div>
            <div className="mempool-stat"><div className="mempool-stat-val">{(mempoolStats.vsize / 1e6).toFixed(1)}MB</div><div className="mempool-stat-lbl">Size</div></div>
            <div className="mempool-stat"><div className="mempool-stat-val">{(mempoolStats.total_fee / 1e8).toFixed(4)}</div><div className="mempool-stat-lbl">Fees (tBTC)</div></div>
          </div>
        )}
        <button className="btn btn-secondary full-width" onClick={handleFetchMempool} disabled={isFetchingMempool}>
          {isFetchingMempool ? 'Loading...' : 'Fetch Mempool'}
        </button>
      </div>
    </aside>
  );
}
