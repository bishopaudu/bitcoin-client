import React from 'react';

export default function AboutModal({ isAboutOpen, setIsAboutOpen }) {
  if (!isAboutOpen) return null;

  return (
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
  );
}
