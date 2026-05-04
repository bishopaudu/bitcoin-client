import React from 'react';

export default function MessageGuideModal({ isGuideOpen, setIsGuideOpen, MESSAGE_GUIDE }) {
  if (!isGuideOpen) return null;

  return (
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
  );
}
