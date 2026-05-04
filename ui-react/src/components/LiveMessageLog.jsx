import React from 'react';

export default function LiveMessageLog({ messages, handleClearLog, logEndRef }) {
  return (
    <div className="card flex-card">
      <div className="card-title">
        Live P2P Message Log
        <button className="btn btn-ghost btn-small" onClick={handleClearLog}>Clear</button>
      </div>
      <div className="message-log">
        {messages.length === 0 && <div className="log-placeholder">Connect to a peer to watch live Bitcoin P2P messages...</div>}
        {messages.map(msg => (
          <div key={msg.id} className="log-entry">
            <span className="log-num">#{String(msg.id).padStart(4, '0')}</span>
            <span className={`log-cmd cmd-${msg.command.toLowerCase()}`}>{msg.command}</span>
            <span className="log-sum">{msg.summary}</span>
          </div>
        ))}
        <div ref={logEndRef} />
      </div>
    </div>
  );
}
