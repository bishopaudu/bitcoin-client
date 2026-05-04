import React from 'react';

export default function LiveNetworkActivity({ txSeenCount, blockSeenCount, invFeed, handleMempoolRowClick }) {
  return (
    <div className="card flex-card">
      <div className="card-title">
        Live Network Activity
        <span className="network-stats">{txSeenCount} TXs · {blockSeenCount} Blocks</span>
      </div>
      <div className="inv-feed">
        {invFeed.length === 0 && <div className="log-placeholder">Waiting for new transactions and blocks to be announced...</div>}
        {invFeed.map(item => (
          <div 
            key={item.id} 
            className={`inv-entry ${item.itemType === 'BLOCK' ? 'inv-block' : 'inv-tx'}`}
            onClick={() => { if(item.itemType === 'TX') handleMempoolRowClick(item.hash); }}
            title={item.itemType === 'TX' ? "Click to load into lookup" : "New block mined!"}
          >
            <span className="inv-type">[{item.itemType}]</span>
            <span className="inv-hash">{item.hash.substring(0, 24)}...</span>
            <span className="inv-time">just now</span>
          </div>
        ))}
      </div>
    </div>
  );
}
