import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import './App.css';

const MEMPOOL_API = 'https://mempool.space/testnet/api';

// ── Message Guide Data ────────────────────────────────────────────────────────
const MESSAGE_GUIDE = [
  { cmd: 'version', color: 'purple', title: 'Version Handshake', description: 'The peer introduces itself. Contains its software version, supported features, block height, and a nonce.', fields: [{ name: 'version', detail: 'Protocol version (e.g. 70016).' }, { name: 'user_agent', detail: 'Software name, e.g. /Satoshi:26.1.0/' }, { name: 'height', detail: "The peer's current block height." }] },
  { cmd: 'verack', color: 'green', title: 'Version Acknowledge', description: 'Confirms the handshake. Once both sides send verack, you are officially a network participant.', fields: [] },
  { cmd: 'ping', color: 'cyan', title: 'Keepalive Ping', description: 'The peer checks if you are still alive. Your app echoes the nonce back as a pong automatically.', fields: [{ name: 'nonce', detail: 'A random 8-byte number echoed back in the pong.' }] },
  { cmd: 'pong', color: 'cyan', title: 'Keepalive Pong', description: 'Your app replied to a ping. The connection is confirmed alive.', fields: [] },
  { cmd: 'sendcmpct', color: 'blue', title: 'Compact Block Negotiation', description: 'The peer supports compact block relay (BIP 152) for bandwidth savings.', fields: [] },
  { cmd: 'feefilter', color: 'orange', title: 'Fee Filter', description: "The peer's minimum fee rate. It will ignore transactions below this threshold (BIP 133).", fields: [{ name: 'fee', detail: 'Minimum fee in sat/kB. 1000 = ~1 sat/byte.' }] },
  { cmd: 'addr', color: 'yellow', title: 'Peer Address List', description: 'A list of other Bitcoin nodes the peer knows about — the network phonebook.', fields: [{ name: 'count', detail: 'Number of addresses. Up to 1000.' }] },
  { cmd: 'inv', color: 'teal', title: 'Inventory Announcement', description: 'The peer announces new transactions or blocks it has. Like a headline — you send getdata to get the full content.', fields: [{ name: 'type', detail: '1=TX, 2=Block, 0x40000001=SegWit TX' }, { name: 'hash', detail: 'TXID or block hash of the announced item.' }] },
  { cmd: 'tx', color: 'green', title: 'Transaction Data', description: 'A full raw transaction sent by the peer.', fields: [] },
  { cmd: 'block', color: 'red', title: 'Block Data', description: 'A full block — 80-byte header plus all transaction data.', fields: [] },
  { cmd: 'notfound', color: 'red', title: 'Not Found', description: "The peer doesn't have the item you requested. Usually because the transaction is confirmed and no longer in the mempool.", fields: [] },
  { cmd: 'reject', color: 'red', title: 'Message Rejected', description: 'The peer rejected something you sent — invalid signature, insufficient fee, or duplicate.', fields: [] },
];

function App() {
  const [showSplash, setShowSplash] = useState(true);

  // Connection state
  const [status, setStatus] = useState('offline');
  const [statusText, setStatusText] = useState('Disconnected');
  const [peerAddress, setPeerAddress] = useState('—');
  const [peerInfo, setPeerInfo] = useState({ agent: '—', height: '—', version: '—', services: '—' });

  // Message log
  const [messages, setMessages] = useState([]);
  const [messageCount, setMessageCount] = useState(0);
  const logEndRef = useRef(null);

  // Transaction lookup (via mempool.space API)
  const [txidInput, setTxidInput] = useState('');
  const [isFetchingTx, setIsFetchingTx] = useState(false);
  const [txData, setTxData] = useState(null);
  const [txError, setTxError] = useState(null);

  // Mempool (via mempool.space API)
  const [isFetchingMempool, setIsFetchingMempool] = useState(false);
  const [mempoolTxids, setMempoolTxids] = useState([]);
  const [mempoolStats, setMempoolStats] = useState(null);

  // Modals
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [isGuideOpen, setIsGuideOpen] = useState(false);

  const isConnected = status === 'online';

  // Splash timer
  useEffect(() => {
    const t = setTimeout(() => setShowSplash(false), 2800);
    return () => clearTimeout(t);
  }, []);

  // P2P Event listeners (connection + live log only)
  useEffect(() => {
    let unlistenConnection, unlistenPeerInfo, unlistenBitcoinMsg;

    async function setupListeners() {
      unlistenConnection = await listen('connection-status', (event) => {
        const { status: s, message, peerAddress: peer } = event.payload;
        if (s === 'handshake_complete') { setStatus('online'); setStatusText('Connected'); }
        else if (s === 'connecting') { setStatus('connecting'); setStatusText(message); if (peer) setPeerAddress(peer); }
        else if (s === 'disconnected') { setStatus('offline'); setStatusText('Disconnected'); setPeerAddress('—'); setPeerInfo({ agent: '—', height: '—', version: '—', services: '—' }); }
        else if (s === 'error') { setStatus('error'); setStatusText('Error: ' + message); }
      });

      unlistenPeerInfo = await listen('peer-info', (event) => {
        const { protocolVersion, userAgent, startHeight, services } = event.payload;
        setPeerInfo({ agent: userAgent, height: startHeight.toLocaleString(), version: protocolVersion, services });
      });

      unlistenBitcoinMsg = await listen('bitcoin-message', (event) => {
        const { command, summary, messageNumber } = event.payload;
        setMessageCount(messageNumber);
        setMessages(prev => [...prev, { id: messageNumber, command, summary }]);
      });
    }

    setupListeners();
    return () => {
      if (unlistenConnection) unlistenConnection();
      if (unlistenPeerInfo) unlistenPeerInfo();
      if (unlistenBitcoinMsg) unlistenBitcoinMsg();
    };
  }, []);

  // Auto-scroll log
  useEffect(() => {
    if (logEndRef.current) logEndRef.current.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // ── Handlers ────────────────────────────────────────────────────────────────
  const handleConnect = async () => {
    setStatus('connecting'); setStatusText('Connecting...');
    try { await invoke('connect'); }
    catch (err) { setStatus('error'); setStatusText('Error: ' + err); }
  };

  const handleDisconnect = async () => {
    try { await invoke('disconnect'); setStatus('offline'); setStatusText('Disconnected'); }
    catch (err) { console.error(err); }
  };

  const handleFetchTx = async () => {
    const txid = txidInput.trim();
    if (txid.length !== 64) { alert('Please enter a valid 64-character TXID'); return; }
    setIsFetchingTx(true);
    setTxData(null);
    setTxError(null);
    try {
      const res = await fetch(`${MEMPOOL_API}/tx/${txid}`);
      if (!res.ok) {
        const msg = res.status === 404
          ? 'Transaction not found. It may not have been broadcast yet or the TXID is incorrect.'
          : `API error: ${res.status}`;
        setTxError(msg);
        setIsModalOpen(true);
      } else {
        const data = await res.json();
        setTxData(data);
        setIsModalOpen(true);
      }
    } catch (err) {
      setTxError('Could not reach mempool.space. Check your internet connection.');
      setIsModalOpen(true);
    } finally {
      setIsFetchingTx(false);
    }
  };

  const handleFetchMempool = async () => {
    setIsFetchingMempool(true);
    setMempoolTxids([]);
    setMempoolStats(null);
    try {
      const [txidsRes, statsRes] = await Promise.all([
        fetch(`${MEMPOOL_API}/mempool/txids`),
        fetch(`${MEMPOOL_API}/mempool`),
      ]);
      if (txidsRes.ok) setMempoolTxids(await txidsRes.json());
      if (statsRes.ok) setMempoolStats(await statsRes.json());
    } catch (err) {
      alert('Could not reach mempool.space. Check your internet connection.');
    } finally {
      setIsFetchingMempool(false);
    }
  };

  const handleMempoolRowClick = (txid) => {
    setTxidInput(txid);
    document.getElementById('txid-input')?.scrollIntoView({ behavior: 'smooth' });
  };

  const handleClearLog = () => { setMessages([]); setMessageCount(0); };

  // ── Helpers ─────────────────────────────────────────────────────────────────
  const satsToTBTC = (sats) => (sats / 1e8).toFixed(8);

  const formatInput = (inp) => {
    if (inp.is_coinbase) return 'COINBASE (block reward)';
    return `${inp.txid.substring(0, 16)}... → output #${inp.vout}`;
  };

  const formatOutput = (out) => {
    const addr = out.scriptpubkey_address || out.scriptpubkey_type || 'unknown';
    return `${satsToTBTC(out.value)} tBTC → ${addr}`;
  };

  // ── Splash ──────────────────────────────────────────────────────────────────
  if (showSplash) {
    return (
      <div className="splash">
        <div className="splash-content">
          <div className="splash-icon">⬡</div>
          <h1 className="splash-title">Cthulhu</h1>
          <p className="splash-subtitle">Bitcoin P2P Network Observer</p>
          <div className="splash-loader"><div className="splash-loader-bar" /></div>
          <p className="splash-network">Testnet3</p>
        </div>
      </div>
    );
  }

  // ── Main App ─────────────────────────────────────────────────────────────────
  return (
    <>
      {/* Top bar */}
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
          <span id="status-text">{statusText}</span>
          <button className="btn btn-primary" onClick={handleConnect} disabled={isConnected || status === 'connecting'}>Connect</button>
          <button className="btn btn-danger" onClick={handleDisconnect} disabled={!isConnected}>Disconnect</button>
        </div>
      </header>

      {/* Layout */}
      <div className="layout">
        {/* Sidebar */}
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

        {/* Main content */}
        <main className="main-content">
          {/* Live log */}
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

          {/* Mempool table */}
          {mempoolTxids.length > 0 && (
            <div className="card">
              <div className="card-title">Mempool — {mempoolTxids.length.toLocaleString()} unconfirmed transactions</div>
              <div className="mempool-table">
                {mempoolTxids.slice(0, 50).map(txid => (
                  <div key={txid} className="mempool-row" title="Click to load into lookup" onClick={() => handleMempoolRowClick(txid)}>
                    {txid}
                  </div>
                ))}
              </div>
              {mempoolTxids.length > 50 && (
                <div className="hint" style={{ marginTop: '8px' }}>Showing 50 of {mempoolTxids.length.toLocaleString()} transactions.</div>
              )}
              <div className="hint" style={{ marginTop: '6px' }}>
                Click any TXID to load it, then click Fetch Transaction to see full details.
              </div>
            </div>
          )}
        </main>
      </div>

      {/* ── Transaction Modal ── */}
      {isModalOpen && (
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
      )}

      {/* ── About Modal ── */}
      {isAboutOpen && (
        <div className="modal-overlay" onClick={() => setIsAboutOpen(false)}>
          <div className="modal-content about-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div><div className="modal-title">About Cthulhu</div><div className="modal-subtitle">Bitcoin P2P Network Observer</div></div>
              <button className="modal-close" onClick={() => setIsAboutOpen(false)}>×</button>
            </div>
            <div className="modal-body">
              <div className="about-logo-row">
                <span className="about-icon">⬡</span>
                <div><div className="about-app-name">Cthulhu</div><div className="about-version">v0.1.0 — Testnet3</div></div>
              </div>
              <p className="about-text">Cthulhu is a Bitcoin Testnet P2P observer built with Rust and React. It connects directly to a Bitcoin node over a raw TCP socket and lets you watch the Bitcoin peer-to-peer protocol in real-time. Transaction and mempool data is fetched from the mempool.space API for reliability.</p>
              <div className="about-section-title">Features</div>
              <ul className="about-list">
                <li>🔗 Connects to live Testnet peers via DNS seed discovery</li>
                <li>🤝 Full Bitcoin version/verack handshake</li>
                <li>📡 Real-time P2P message stream</li>
                <li>🔍 Transaction lookup for any TX (confirmed or unconfirmed)</li>
                <li>📸 Mempool snapshot with live unconfirmed transactions</li>
              </ul>
              <div className="about-section-title">Tech Stack</div>
              <div className="about-tags">
                {['Rust', 'Tauri', 'React', 'Vite', 'Bitcoin P2P', 'Testnet3', 'mempool.space'].map(t => (
                  <span key={t} className="about-tag">{t}</span>
                ))}
              </div>
              <div className="about-footer">Built with ❤️ — Raw Bitcoin. No shortcuts.</div>
            </div>
          </div>
        </div>
      )}

      {/* ── Message Guide Modal ── */}
      {isGuideOpen && (
        <div className="modal-overlay" onClick={() => setIsGuideOpen(false)}>
          <div className="modal-content guide-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div><div className="modal-title">Bitcoin P2P Message Guide</div><div className="modal-subtitle">What every message in the live log means</div></div>
              <button className="modal-close" onClick={() => setIsGuideOpen(false)}>×</button>
            </div>
            <div className="modal-body">
              {MESSAGE_GUIDE.map(entry => (
                <div key={entry.cmd} className="guide-entry">
                  <div className="guide-entry-header">
                    <span className={`log-cmd cmd-${entry.cmd}`}>{entry.cmd}</span>
                    <span className="guide-title">{entry.title}</span>
                  </div>
                  <p className="guide-desc">{entry.description}</p>
                  {entry.fields.length > 0 && (
                    <div className="guide-fields">
                      {entry.fields.map(f => (
                        <div key={f.name} className="guide-field">
                          <span className="guide-field-name">{f.name}</span>
                          <span className="guide-field-detail">{f.detail}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export default App;
