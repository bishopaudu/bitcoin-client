import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import './App.css';

// Components
import Topbar from './components/Topbar';
import Sidebar from './components/Sidebar';
import LiveMessageLog from './components/LiveMessageLog';
import LiveNetworkActivity from './components/LiveNetworkActivity';
import TransactionModal from './components/Modals/TransactionModal';
import AboutModal from './components/Modals/AboutModal';
import MessageGuideModal from './components/Modals/MessageGuideModal';

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

  // Live Inv Feed
  const [invFeed, setInvFeed] = useState([]);
  const [txSeenCount, setTxSeenCount] = useState(0);
  const [blockSeenCount, setBlockSeenCount] = useState(0);

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

  // P2P Event listeners
  useEffect(() => {
    let unlistenConnection, unlistenPeerInfo, unlistenBitcoinMsg, unlistenInv;

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
      
      unlistenInv = await listen('inv-announcement', (event) => {
        const { items, timestamp } = event.payload;
        
        let newTxs = 0;
        let newBlocks = 0;
        
        const newFeedItems = items.map(item => {
            if (item.itemCase === "TX") newTxs++;
            if (item.itemCase === "BLOCK") newBlocks++;
            // The rust struct used rename_all="camelCase", so it's itemType
            if (item.itemType === "TX") newTxs++;
            if (item.itemType === "BLOCK") newBlocks++;
            return {
                ...item,
                id: item.hash + '-' + Math.random().toString(36).substr(2, 5),
                time: timestamp
            };
        });

        setTxSeenCount(prev => prev + newTxs);
        setBlockSeenCount(prev => prev + newBlocks);
        
        setInvFeed(prev => {
            const next = [...newFeedItems, ...prev];
            return next.slice(0, 200); // Cap at 200 items to prevent memory bloat
        });
      });
    }

    setupListeners();
    return () => {
      if (unlistenConnection) unlistenConnection();
      if (unlistenPeerInfo) unlistenPeerInfo();
      if (unlistenBitcoinMsg) unlistenBitcoinMsg();
      if (unlistenInv) unlistenInv();
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
      <Topbar 
        status={status}
        statusText={statusText}
        isConnected={isConnected}
        peerAddress={peerAddress}
        handleConnect={handleConnect}
        handleDisconnect={handleDisconnect}
        setIsGuideOpen={setIsGuideOpen}
        setIsAboutOpen={setIsAboutOpen}
      />

      {/* Layout */}
      <div className="layout">
        <Sidebar 
          peerAddress={peerAddress}
          peerInfo={peerInfo}
          messageCount={messageCount}
          txidInput={txidInput}
          setTxidInput={setTxidInput}
          handleFetchTx={handleFetchTx}
          isFetchingTx={isFetchingTx}
          mempoolStats={mempoolStats}
          handleFetchMempool={handleFetchMempool}
          isFetchingMempool={isFetchingMempool}
        />

        {/* Main content */}
        <main className="main-content">
          <div className="log-grid">
            <LiveMessageLog 
              messages={messages}
              handleClearLog={handleClearLog}
              logEndRef={logEndRef}
            />

            <LiveNetworkActivity 
              txSeenCount={txSeenCount}
              blockSeenCount={blockSeenCount}
              invFeed={invFeed}
              handleMempoolRowClick={handleMempoolRowClick}
            />
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

      <TransactionModal 
        isModalOpen={isModalOpen}
        setIsModalOpen={setIsModalOpen}
        txError={txError}
        txData={txData}
        satsToTBTC={satsToTBTC}
        formatInput={formatInput}
      />

      <AboutModal 
        isAboutOpen={isAboutOpen}
        setIsAboutOpen={setIsAboutOpen}
      />

      <MessageGuideModal 
        isGuideOpen={isGuideOpen}
        setIsGuideOpen={setIsGuideOpen}
        MESSAGE_GUIDE={MESSAGE_GUIDE}
      />
    </>
  );
}

export default App;
