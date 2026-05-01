import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import './App.css';

// ── Message type reference data ───────────────────────────────────────────────
const MESSAGE_GUIDE = [
  {
    cmd: 'version',
    color: 'purple',
    title: 'Version Handshake',
    description: 'The peer is introducing itself. Contains its software version, supported features (services), current block height, and a unique nonce. Your app replies with a verack to acknowledge.',
    fields: [
      { name: 'version', detail: 'Protocol version (e.g. 70016). Both nodes must agree.' },
      { name: 'services', detail: 'Bitmask of features. 0x409 means Full Node + SegWit + Addr support.' },
      { name: 'user_agent', detail: 'Software name, e.g. /Satoshi:26.1.0/ is Bitcoin Core 26.1.' },
      { name: 'height', detail: 'The peer\'s current block height — how synced they are.' },
    ],
  },
  {
    cmd: 'verack',
    color: 'green',
    title: 'Version Acknowledge',
    description: 'The peer is confirming it received your version message. Once both sides send verack, the Bitcoin handshake is complete and you are officially a network participant.',
    fields: [],
  },
  {
    cmd: 'ping',
    color: 'cyan',
    title: 'Keepalive Ping',
    description: 'The peer is checking if you are still alive. Your app automatically reads the nonce and echoes it back as a pong. If you do not reply, the peer will eventually disconnect you.',
    fields: [
      { name: 'nonce', detail: 'A random 8-byte number. You must echo this exact value back in your pong.' },
    ],
  },
  {
    cmd: 'pong',
    color: 'cyan',
    title: 'Keepalive Pong',
    description: 'Your app sent a pong in response to a ping. This confirms your connection is still alive.',
    fields: [],
  },
  {
    cmd: 'sendcmpct',
    color: 'blue',
    title: 'Compact Block Negotiation',
    description: 'The peer supports compact block relay (BIP 152). Instead of sending entire blocks, it sends a compressed version assuming you already have most transactions. Saves bandwidth.',
    fields: [],
  },
  {
    cmd: 'feefilter',
    color: 'orange',
    title: 'Fee Filter',
    description: 'The peer is telling you its minimum fee rate threshold. It will silently ignore any transactions you send with a fee rate below this value — a spam protection mechanism (BIP 133).',
    fields: [
      { name: 'fee', detail: 'Minimum fee in satoshis per kilobyte. 1000 sat/kB = ~1 sat/byte.' },
    ],
  },
  {
    cmd: 'addr',
    color: 'yellow',
    title: 'Peer Address List',
    description: 'The peer is sharing a list of other Bitcoin nodes it knows about. This is how peer discovery works — like a phonebook of the network. Your app requested this with a getaddr message after the handshake.',
    fields: [
      { name: 'count', detail: 'Number of node addresses included. Can be up to 1000.' },
    ],
  },
  {
    cmd: 'inv',
    color: 'teal',
    title: 'Inventory Announcement',
    description: 'The peer is announcing new items it has — either new transactions or new blocks. Think of it as a newspaper headline. You can then send a getdata to request the full content.',
    fields: [
      { name: 'type', detail: 'Item type: 1=Transaction, 2=Block, 0x40000001=SegWit Transaction.' },
      { name: 'hash', detail: 'The unique identifier (TXID or block hash) of the announced item.' },
    ],
  },
  {
    cmd: 'tx',
    color: 'green',
    title: 'Transaction Data',
    description: 'The peer sent you a full transaction in response to your getdata request. Your app parses it and displays inputs, outputs, total BTC, and locktime in the Transaction Modal.',
    fields: [
      { name: 'txid', detail: 'The transaction\'s unique ID (double-SHA256 of the raw bytes, reversed).' },
      { name: 'inputs', detail: 'Which previous transaction outputs are being spent.' },
      { name: 'outputs', detail: 'Where the BTC is going. Each has a value and a locking script.' },
    ],
  },
  {
    cmd: 'block',
    color: 'red',
    title: 'Block Data',
    description: 'A full block containing many transactions. Contains an 80-byte header (version, previous block hash, merkle root, timestamp, difficulty target, nonce) plus all transaction data.',
    fields: [],
  },
  {
    cmd: 'getdata',
    color: 'blue',
    title: 'Data Request (Outgoing)',
    description: 'Your app sent this to the peer requesting a specific transaction. This is the message sent when you click "Fetch Transaction". The peer responds with a tx message if it has it, or a notfound message if it does not.',
    fields: [],
  },
  {
    cmd: 'notfound',
    color: 'red',
    title: 'Not Found',
    description: 'The peer does not have the item you requested. This usually means the transaction is already confirmed in a block and is no longer in the peer\'s mempool. Peers only keep unconfirmed transactions unless they have a full transaction index enabled.',
    fields: [],
  },
  {
    cmd: 'reject',
    color: 'red',
    title: 'Message Rejected',
    description: 'The peer explicitly rejected something you sent. This could be a transaction with an invalid signature, insufficient fee, or a duplicate. Contains an error code and reason string.',
    fields: [],
  },
];

function App() {
  const [showSplash, setShowSplash] = useState(true);

  const [status, setStatus] = useState('offline');
  const [statusText, setStatusText] = useState('Disconnected');
  const [peerAddress, setPeerAddress] = useState('—');
  const [peerInfo, setPeerInfo] = useState({ agent: '—', height: '—', version: '—', services: '—' });

  const [messages, setMessages] = useState([]);
  const [messageCount, setMessageCount] = useState(0);
  const logEndRef = useRef(null);

  const [txidInput, setTxidInput] = useState('');
  const [isFetchingTx, setIsFetchingTx] = useState(false);
  const [txData, setTxData] = useState(null);

  const [isFetchingMempool, setIsFetchingMempool] = useState(false);
  const [mempoolData, setMempoolData] = useState(null);

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAboutOpen, setIsAboutOpen] = useState(false);
  const [isGuideOpen, setIsGuideOpen] = useState(false);

  const isConnected = status === 'online';

  // ── Splash screen timer ───────────────────────────────────────────────────
  useEffect(() => {
    const timer = setTimeout(() => setShowSplash(false), 2800);
    return () => clearTimeout(timer);
  }, []);

  // ── Event listeners ───────────────────────────────────────────────────────
  useEffect(() => {
    let unlistenConnection;
    let unlistenPeerInfo;
    let unlistenBitcoinMsg;
    let unlistenTx;
    let unlistenMempool;
    let unlistenNotfound;

    async function setupListeners() {
      unlistenConnection = await listen('connection-status', (event) => {
        const { status: newStatus, message, peerAddress: peer } = event.payload;
        if (newStatus === 'handshake_complete') {
          setStatus('online');
          setStatusText('Connected');
        } else if (newStatus === 'connecting') {
          setStatus('connecting');
          setStatusText(message);
          if (peer) setPeerAddress(peer);
        } else if (newStatus === 'disconnected') {
          setStatus('offline');
          setStatusText('Disconnected');
        } else if (newStatus === 'error') {
          setStatus('error');
          setStatusText('Error: ' + message);
        }
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

      unlistenTx = await listen('transaction-decoded', (event) => {
        setTxData(event.payload);
        setIsFetchingTx(false);
        setIsModalOpen(true);
      });

      unlistenNotfound = await listen('transaction-notfound', () => {
        alert('Transaction not found! The peer does not have this TX in its mempool.');
        setIsFetchingTx(false);
      });

      unlistenMempool = await listen('mempool-snapshot', (event) => {
        setMempoolData(event.payload);
        setIsFetchingMempool(false);
      });
    }

    setupListeners();

    return () => {
      if (unlistenConnection) unlistenConnection();
      if (unlistenPeerInfo) unlistenPeerInfo();
      if (unlistenBitcoinMsg) unlistenBitcoinMsg();
      if (unlistenTx) unlistenTx();
      if (unlistenMempool) unlistenMempool();
      if (unlistenNotfound) unlistenNotfound();
    };
  }, []);

  // Auto-scroll log
  useEffect(() => {
    if (logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  const handleConnect = async () => {
    setStatus('connecting');
    setStatusText('Connecting...');
    try {
      await invoke('connect');
    } catch (err) {
      setStatus('error');
      setStatusText('Error: ' + err);
    }
  };

  const handleDisconnect = async () => {
    try {
      await invoke('disconnect');
      setStatus('offline');
      setStatusText('Disconnected');
    } catch (err) {
      console.error('Disconnect error:', err);
    }
  };

  const handleFetchTx = async () => {
    const txid = txidInput.trim();
    if (txid.length !== 64) {
      alert('Please enter a valid 64-character TXID');
      return;
    }
    setIsFetchingTx(true);
    setMessages(prev => [...prev, {
      id: 'out-' + Date.now(),
      command: 'getdata',
      summary: `Requested TX ${txid.substring(0, 16)}...`
    }]);
    try {
      await invoke('request_tx', { txid_hex: txid });
      setTimeout(() => {
        setIsFetchingTx(prev => {
          if (prev) alert("Request timed out! The peer ignored us — it likely doesn't have this transaction in its mempool.");
          return false;
        });
      }, 5000);
    } catch (err) {
      alert('Error: ' + err);
      setIsFetchingTx(false);
    }
  };

  const handleFetchMempool = async () => {
    setIsFetchingMempool(true);
    try {
      await invoke('request_mempool');
    } catch (err) {
      alert('Error: ' + err);
      setIsFetchingMempool(false);
    }
  };

  const handleClearLog = () => {
    setMessages([]);
    setMessageCount(0);
  };

  const handleMempoolRowClick = (txid) => {
    setTxidInput(txid);
    document.getElementById('txid-input')?.scrollIntoView({ behavior: 'smooth' });
  };

  // ── Splash Screen ─────────────────────────────────────────────────────────
  if (showSplash) {
    return (
      <div className="splash">
        <div className="splash-content">
          <div className="splash-icon">⬡</div>
          <h1 className="splash-title">Cthulhu</h1>
          <p className="splash-subtitle">Bitcoin P2P Network Observer</p>
          <div className="splash-loader">
            <div className="splash-loader-bar" />
          </div>
          <p className="splash-network">Testnet3</p>
        </div>
      </div>
    );
  }

  // ── Main App ──────────────────────────────────────────────────────────────
  return (
    <>
      {/* ── Top bar ── */}
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

      {/* ── Main layout ── */}
      <div className="layout">

        {/* Left sidebar */}
        <aside className="sidebar">
          <div className="card">
            <div className="card-title">Peer Info</div>
            <div className="info-row"><span className="label">Address</span><span>{peerAddress}</span></div>
            <div className="info-row"><span className="label">Agent</span><span>{peerInfo.agent}</span></div>
            <div className="info-row"><span className="label">Height</span><span>{peerInfo.height}</span></div>
            <div className="info-row"><span className="label">Protocol</span><span>{peerInfo.version}</span></div>
            <div className="info-row"><span className="label">Services</span><span>{peerInfo.services}</span></div>
            <div className="info-row"><span className="label">Messages</span><span>{messageCount}</span></div>
          </div>

          <div className="card">
            <div className="card-title">Look Up Transaction</div>
            <input
              id="txid-input"
              className="text-input"
              placeholder="Paste 64-char TXID here..."
              maxLength="64"
              value={txidInput}
              onChange={(e) => setTxidInput(e.target.value)}
            />
            <button className="btn btn-primary full-width" onClick={handleFetchTx} disabled={!isConnected || isFetchingTx || txidInput.trim().length !== 64}>
              {isFetchingTx ? 'Requesting...' : 'Fetch Transaction'}
            </button>
          </div>

          <div className="card">
            <div className="card-title">Mempool Snapshot</div>
            <p className="hint">Request all unconfirmed transactions currently in the peer's mempool.</p>
            <button className="btn btn-secondary full-width" onClick={handleFetchMempool} disabled={!isConnected || isFetchingMempool}>
              {isFetchingMempool ? 'Waiting for response...' : 'Fetch Mempool'}
            </button>
            {mempoolData && <div className="mempool-count">{mempoolData.totalCount.toLocaleString()} unconfirmed TXs found</div>}
          </div>
        </aside>

        {/* Right main */}
        <main className="main-content">
          <div className="card flex-card">
            <div className="card-title">
              Live Message Log
              <button className="btn btn-ghost btn-small" onClick={handleClearLog}>Clear</button>
            </div>
            <div className="message-log">
              {messages.length === 0 && <div className="log-placeholder">Connect to a peer to see live Bitcoin P2P messages...</div>}
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

          {mempoolData && (
            <div className="card">
              <div className="card-title">Mempool — <span>{mempoolData.totalCount.toLocaleString()}</span> unconfirmed transactions</div>
              <div className="mempool-table">
                {mempoolData.txids.map(txid => (
                  <div key={txid} className="mempool-row" title="Click to load into lookup" onClick={() => handleMempoolRowClick(txid)}>
                    {txid}
                  </div>
                ))}
              </div>
              <div className="hint" style={{ marginTop: '8px' }}>
                Tip: look up any of these on <a href="https://mempool.space/testnet" target="_blank" rel="noreferrer">mempool.space/testnet</a>
              </div>
            </div>
          )}
        </main>
      </div>

      {/* ── Transaction Modal ── */}
      {isModalOpen && txData && (
        <div className="modal-overlay" onClick={() => setIsModalOpen(false)}>
          <div className="modal-content" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div>
                <div className="modal-title">Transaction Details</div>
                <div className="modal-subtitle">Fetched from node: {txData.fetchedFrom || peerAddress}</div>
              </div>
              <button className="modal-close" onClick={() => setIsModalOpen(false)}>×</button>
            </div>
            <div className="modal-body">
              <div className="tx-header">
                <div className="tx-txid">{txData.txid}</div>
                <div className="tx-badges">
                  <span className="badge">v{txData.version}</span>
                  {txData.isSegwit && <span className="badge badge-blue">SegWit</span>}
                </div>
              </div>
              <div className="tx-stats">
                <div className="stat"><div className="stat-value">{txData.inputCount}</div><div className="stat-label">Inputs</div></div>
                <div className="stat"><div className="stat-value">{txData.outputCount}</div><div className="stat-label">Outputs</div></div>
                <div className="stat"><div className="stat-value">{(txData.totalOutputSats / 1e8).toFixed(8)} BTC</div><div className="stat-label">Total BTC</div></div>
                <div className="stat"><div className="stat-value">{txData.locktime}</div><div className="stat-label">Locktime</div></div>
              </div>
              <div className="tx-section-title">Inputs</div>
              <div className="tx-list">
                {txData.inputs.map((inp, i) => <div key={i} className="tx-row">{inp}</div>)}
              </div>
              <div className="tx-section-title">Outputs</div>
              <div className="tx-list">
                {txData.outputs.map((out, i) => <div key={i} className="tx-row">{out}</div>)}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ── About Modal ── */}
      {isAboutOpen && (
        <div className="modal-overlay" onClick={() => setIsAboutOpen(false)}>
          <div className="modal-content about-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div>
                <div className="modal-title">About Cthulhu</div>
                <div className="modal-subtitle">Bitcoin P2P Network Observer</div>
              </div>
              <button className="modal-close" onClick={() => setIsAboutOpen(false)}>×</button>
            </div>
            <div className="modal-body">
              <div className="about-logo-row">
                <span className="about-icon">⬡</span>
                <div>
                  <div className="about-app-name">Cthulhu</div>
                  <div className="about-version">v0.1.0 — Testnet3</div>
                </div>
              </div>

              <p className="about-text">
                Cthulhu is a low-level Bitcoin P2P network observer built with Rust and React.
                It connects directly to a Bitcoin Testnet node over a raw TCP socket — no API,
                no middleman — and lets you watch the real Bitcoin peer-to-peer protocol in action.
              </p>

              <div className="about-section-title">What it does</div>
              <ul className="about-list">
                <li>🔗 Connects to a live Testnet peer via DNS seed discovery</li>
                <li>🤝 Performs the full Bitcoin version/verack handshake</li>
                <li>📡 Streams every incoming P2P message in real-time</li>
                <li>🔍 Fetches and decodes unconfirmed transactions from the mempool</li>
                <li>📸 Takes a mempool snapshot showing all pending transactions</li>
                <li>🧬 Parses both Legacy and SegWit transaction formats</li>
              </ul>

              <div className="about-section-title">How it works</div>
              <p className="about-text">
                The Rust backend runs a Bitcoin P2P client on a background thread so the UI
                never freezes. It emits structured events across the Tauri bridge, which React
                receives and renders in real-time. All Bitcoin wire-format parsing — VarInts,
                CompactSize, SegWit marker bytes — is implemented from scratch in Rust.
              </p>

              <div className="about-section-title">Tech Stack</div>
              <div className="about-tags">
                <span className="about-tag">Rust</span>
                <span className="about-tag">Tauri</span>
                <span className="about-tag">React</span>
                <span className="about-tag">Vite</span>
                <span className="about-tag">Bitcoin P2P</span>
                <span className="about-tag">Testnet3</span>
              </div>

              <div className="about-footer">
                Built with ❤️ — Raw Bitcoin. No shortcuts.
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ── Message Guide Modal ── */}
      {isGuideOpen && (
        <div className="modal-overlay" onClick={() => setIsGuideOpen(false)}>
          <div className="modal-content guide-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div>
                <div className="modal-title">Bitcoin P2P Message Guide</div>
                <div className="modal-subtitle">What every message in the live log means</div>
              </div>
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
