import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import './App.css';

function App() {
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

  const isConnected = status === 'online';

  // Setup event listeners
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
    
    // Optimistically add an outgoing log message so the user gets instant visual feedback
    setMessages(prev => [...prev, { 
      id: 'out', 
      command: 'getdata', 
      summary: `Requested TX ${txid.substring(0, 16)}...` 
    }]);

    try {
      // Must exactly match the Rust parameter name `txid_hex`
      await invoke('request_tx', { txid_hex: txid });
      
      // Bitcoin nodes often silently ignore requests if they don't have the TX.
      // So we MUST have a timeout fallback just in case they ghost us.
      setTimeout(() => {
        setIsFetchingTx(prev => {
          if (prev) alert("Request timed out! The peer ignored us because it doesn't have this transaction in its mempool.");
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

  return (
    <>
      {/* ── Top bar ── */}
      <header className="topbar">
        <div className="topbar-left">
          <span className="logo">₿</span>
          <span className="app-title">Bitcoin P2P Observer</span>
          <span className="network-badge">Testnet3</span>
        </div>
        <div className="topbar-right">
          <span className={`dot dot-${status}`}></span>
          <span id="status-text">{statusText}</span>
          <button className="btn btn-primary" onClick={handleConnect} disabled={isConnected || status === 'connecting'}>Connect</button>
          <button className="btn btn-danger" onClick={handleDisconnect} disabled={!isConnected}>Disconnect</button>
        </div>
      </header>

      {/* ── Main layout: two columns ── */}
      <div className="layout">

        {/* Left column: peer info + controls */}
        <aside className="sidebar">

          {/* Peer info card */}
          <div className="card">
            <div className="card-title">Peer Info</div>
            <div className="info-row"><span className="label">Address</span><span>{peerAddress}</span></div>
            <div className="info-row"><span className="label">Agent</span><span>{peerInfo.agent}</span></div>
            <div className="info-row"><span className="label">Height</span><span>{peerInfo.height}</span></div>
            <div className="info-row"><span className="label">Protocol</span><span>{peerInfo.version}</span></div>
            <div className="info-row"><span className="label">Services</span><span>{peerInfo.services}</span></div>
            <div className="info-row"><span className="label">Messages</span><span>{messageCount}</span></div>
          </div>

          {/* TX lookup */}
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

          {/* Mempool */}
          <div className="card">
            <div className="card-title">Mempool Snapshot</div>
            <p className="hint">Request all unconfirmed transactions currently in the peer's mempool.</p>
            <button className="btn btn-secondary full-width" onClick={handleFetchMempool} disabled={!isConnected || isFetchingMempool}>
              {isFetchingMempool ? 'Waiting for response...' : 'Fetch Mempool'}
            </button>
            {mempoolData && <div className="mempool-count">{mempoolData.totalCount.toLocaleString()} unconfirmed TXs found</div>}
          </div>

        </aside>

        {/* Right column: message log + tx detail + mempool table */}
        <main className="main-content">

          {/* Live message log */}
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

          {/* Mempool table */}
          {mempoolData && (
            <div className="card">
              <div className="card-title">Mempool — <span>{mempoolData.totalCount.toLocaleString()}</span> unconfirmed transactions</div>
              <div className="mempool-table">
                {mempoolData.txids.map(txid => (
                  <div key={txid} className="mempool-row" title="Click to look up" onClick={() => handleMempoolRowClick(txid)}>
                    {txid}
                  </div>
                ))}
              </div>
              <div className="hint" style={{ marginTop: '8px' }}>
                Tip: look up any of these on <a href={`https://mempool.space/testnet`} target="_blank" rel="noreferrer">mempool.space/testnet</a>
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
    </>
  );
}

export default App;
