import React from 'react';

export default function TransactionModal({
  isModalOpen,
  setIsModalOpen,
  txError,
  txData,
  satsToTBTC,
  formatInput
}) {
  if (!isModalOpen) return null;

  return (
    <div className="modal-overlay" onClick={() => setIsModalOpen(false)}>
      <div className="modal-content" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <div>
            <div className="modal-title">Transaction Details</div>
            <div className="modal-subtitle">Source: mempool.space Testnet API</div>
          </div>
          <button className="modal-close" onClick={() => setIsModalOpen(false)}>×</button>
        </div>
        <div className="modal-body">
          {txError ? (
            <div className="tx-error">
              <div className="tx-error-icon">⚠</div>
              <div className="tx-error-msg">{txError}</div>
            </div>
          ) : txData ? (
            <>
              {/* TXID + status */}
              <div className="tx-header">
                <div className="tx-txid">{txData.txid}</div>
                <div className="tx-badges">
                  <span className="badge">v{txData.version}</span>
                  {txData.status?.confirmed
                    ? <span className="badge badge-green">✓ Confirmed — Block #{txData.status.block_height}</span>
                    : <span className="badge badge-orange">⏳ Unconfirmed</span>
                  }
                </div>
              </div>

              {/* Stats */}
              <div className="tx-stats">
                <div className="stat"><div className="stat-value">{txData.vin?.length}</div><div className="stat-label">Inputs</div></div>
                <div className="stat"><div className="stat-value">{txData.vout?.length}</div><div className="stat-label">Outputs</div></div>
                <div className="stat"><div className="stat-value">{txData.fee?.toLocaleString()}</div><div className="stat-label">Fee (sats)</div></div>
                <div className="stat"><div className="stat-value">{txData.size}</div><div className="stat-label">Bytes</div></div>
              </div>

              {/* Inputs */}
              <div className="tx-section-title">Inputs</div>
              <div className="tx-list">
                {txData.vin?.map((inp, i) => (
                  <div key={i} className="tx-row">
                    <div className="tx-row-label">{formatInput(inp)}</div>
                    {inp.prevout && <div className="tx-row-value">{satsToTBTC(inp.prevout.value)} tBTC · {inp.prevout.scriptpubkey_type}</div>}
                  </div>
                ))}
              </div>

              {/* Outputs */}
              <div className="tx-section-title">Outputs</div>
              <div className="tx-list">
                {txData.vout?.map((out, i) => (
                  <div key={i} className="tx-row">
                    <div className="tx-row-label">{out.scriptpubkey_address || out.scriptpubkey_type}</div>
                    <div className="tx-row-value">{satsToTBTC(out.value)} tBTC</div>
                  </div>
                ))}
              </div>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}
