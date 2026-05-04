import React from 'react';

export default function Topbar({
  status,
  statusText,
  isConnected,
  peerAddress,
  handleConnect,
  handleDisconnect,
  setIsGuideOpen,
  setIsAboutOpen
}) {
  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="logo">⬡</span>
        <span className="app-title">Cthulhu</span>
        <span className="network-badge">Testnet3</span>
      </div>
      <div className="topbar-right">
        <button className="btn btn-ghost" onClick={() => setIsGuideOpen(true)}>Message Guide</button>
        <button className="btn btn-ghost" onClick={() => setIsAboutOpen(true)}>About</button>
        <span className={`dot dot-${status}`}></span>
        <span id="status-text">
          {statusText} {isConnected && peerAddress !== '—' && <span className="peer-ip">({peerAddress})</span>}
        </span>
        <button className="btn btn-primary" onClick={handleConnect} disabled={isConnected || status === 'connecting'}>Connect</button>
        <button className="btn btn-danger" onClick={handleDisconnect} disabled={!isConnected}>Disconnect</button>
      </div>
    </header>
  );
}
