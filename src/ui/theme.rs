// Theme constants — used in inline styles
// Dioxus uses CSS strings, so these are string constants

pub const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
    background: #0c0c0c;
    color: #aaa;
    overflow: hidden;
    height: 100vh;
}

.app { display: flex; flex-direction: column; height: 100vh; }

/* ── Topbar ── */
.topbar {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px; background: #080808;
    border-bottom: 1px solid #1a1a1a; flex-shrink: 0;
}
.tb-server {
    width: 36px; height: 36px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
    font-size: 13px; font-weight: 500; cursor: pointer;
    background: #141414; color: #555; border: none;
    transition: all .15s;
}
.tb-server:hover { background: #1e1e1e; color: #999; }
.tb-server.active { background: #1a1a1a; color: #e0e0e0; outline: 1.5px solid #333; outline-offset: 1px; }
.tb-sep { width: 1px; height: 24px; background: #1a1a1a; }
.tb-add { background: transparent; border: 1.5px dashed #222; color: #333; font-size: 18px; cursor: pointer; }
.tb-add:hover { border-color: #444; color: #666; }
.tb-spacer { flex: 1; }
.tb-identity { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: 6px; cursor: pointer; }
.tb-identity:hover { background: #141414; }
.tb-dot { width: 6px; height: 6px; border-radius: 50%; background: #22c55e; }

/* ── Sidebar ── */
.sidebar {
    width: 240px; background: #0c0c0c; display: flex; flex-direction: column;
    flex-shrink: 0; border-right: 1px solid #1a1a1a;
}
.sidebar-header {
    padding: 10px 14px; border-bottom: 1px solid #1a1a1a;
    font-size: 14px; font-weight: 500; color: #e0e0e0;
}
.channels { flex: 1; overflow-y: auto; padding: 6px 0; }
.ch-section {
    font-size: 10px; font-weight: 600; color: #333; letter-spacing: 1px;
    padding: 12px 14px 4px; text-transform: uppercase;
    display: flex; align-items: center; justify-content: space-between;
}
.ch-item {
    display: flex; align-items: center; gap: 7px; padding: 4px 14px;
    font-size: 13px; color: #4a4a4a; cursor: pointer; transition: all .1s; border: none; background: none; width: 100%; text-align: left;
}
.ch-item:hover { color: #aaa; background: rgba(255,255,255,0.02); }
.ch-item.active { color: #e0e0e0; background: rgba(255,255,255,0.04); }
.ch-prefix { font-size: 12px; color: #2a2a2a; width: 14px; }
.ch-item.active .ch-prefix { color: #555; }
.ch-unread { width: 5px; height: 5px; border-radius: 50%; background: #e0e0e0; margin-left: auto; }

/* ── Main area ── */
.main { flex: 1; display: flex; flex-direction: column; background: #101010; min-width: 0; }
.main-header {
    display: flex; align-items: center; padding: 10px 20px;
    border-bottom: 1px solid #1a1a1a; flex-shrink: 0; gap: 8px;
}
.mh-channel { font-size: 14px; color: #e0e0e0; font-weight: 500; }
.mh-sep { width: 1px; height: 14px; background: #1a1a1a; margin: 0 8px; }
.mh-meta { font-size: 11px; color: #333; }
.mh-spacer { flex: 1; }

.messages { flex: 1; overflow-y: auto; padding: 8px 0; }

.msg { display: flex; gap: 10px; padding: 5px 24px; }
.msg:hover { background: rgba(255,255,255,0.008); }
.msg-avatar {
    width: 34px; height: 34px; border-radius: 8px; background: #141414;
    display: flex; align-items: center; justify-content: center;
    font-size: 12px; font-weight: 500; color: #555; flex-shrink: 0;
}
.msg-name { font-size: 12px; font-weight: 500; color: #bbb; }
.msg-time { font-size: 9px; color: #2a2a2a; margin-left: 6px; }
.msg-e2e { font-size: 8px; color: #1e1e1e; background: #141414; padding: 0 4px; border-radius: 2px; margin-left: 4px; }
.msg-edited { font-size: 9px; color: #333; margin-left: 4px; }
.msg-text { font-size: 13px; color: #777; margin-top: 3px; line-height: 1.5; }
.msg-cont { padding: 1px 24px 1px 68px; }

/* Markdown in messages */
.msg-text strong { color: #bbb; font-weight: 600; }
.msg-text em { color: #888; font-style: italic; }
.msg-text code { background: #1a1a1a; color: #aaa; padding: 1px 4px; border-radius: 3px; font-family: monospace; font-size: 12px; }
.msg-text a { color: #3b82f6; text-decoration: none; }
.msg-text a:hover { text-decoration: underline; }

.msg-reply { font-size: 10px; color: #444; background: #141414; padding: 2px 8px; border-radius: 3px; margin-bottom: 4px; }
.msg-attachment { padding: 6px 10px; background: #141414; border: 1px solid #1a1a1a; border-radius: 4px; font-size: 12px; color: #bbb; margin-top: 4px; }

.reactions { display: flex; gap: 4px; margin-top: 4px; }
.reaction {
    font-size: 10px; color: #777; background: #141414; padding: 2px 6px;
    border-radius: 4px; cursor: pointer; border: none;
}
.reaction:hover { background: #1e1e1e; color: #bbb; }

/* ── Input ── */
.input-area { padding: 0 20px 16px; }
.reply-banner {
    display: flex; align-items: center; padding: 4px 20px;
    background: #141414; font-size: 11px; color: #555;
}
.reply-banner button { background: none; border: none; color: #444; cursor: pointer; margin-left: auto; }
.input-box {
    display: flex; align-items: center; background: #141414;
    border-radius: 8px; border: 1px solid #1a1a1a; padding: 2px 4px;
}
.input-box:focus-within { border-color: #2a2a2a; }
.input-box input {
    flex: 1; background: none; border: none; outline: none;
    color: #bbb; font-size: 13px; padding: 9px 6px; font-family: inherit;
}
.input-box input::placeholder { color: #222; }

/* ── Members panel ── */
.members {
    width: 200px; background: #0c0c0c; border-left: 1px solid #1a1a1a;
    flex-shrink: 0; overflow-y: auto; padding: 0 8px;
}
.member {
    display: flex; align-items: center; gap: 7px; padding: 4px 12px;
    cursor: pointer; border-radius: 0; border: none; background: none; width: 100%;
}
.member:hover { background: rgba(255,255,255,0.015); }
.member .m-avatar {
    width: 26px; height: 26px; border-radius: 6px; background: #141414;
    display: flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 500; color: #555;
}
.member .m-name { font-size: 11px; color: #666; }
.member .m-status { font-size: 9px; color: #2a2a2a; }
.member.offline .m-avatar { opacity: 0.25; }
.member.offline .m-name { color: #2a2a2a; }

.status-dot { width: 6px; height: 6px; border-radius: 50%; display: inline-block; }
.status-dot.online { background: #22c55e; }
.status-dot.away { background: #f59e0b; }
.status-dot.dnd { background: #ef4444; }
.status-dot.offline { background: #333; }

/* ── Voice panel ── */
.voice-bar {
    border-top: 1px solid rgba(34,197,94,0.1); padding: 8px 14px;
    background: rgba(34,197,94,0.03);
}
.voice-bar .vb-status { font-size: 10px; color: #22c55e; }
.voice-bar .vb-controls { display: flex; gap: 3px; margin-top: 6px; }
.vb-btn {
    font-size: 10px; padding: 4px 8px; border-radius: 4px;
    border: 1px solid #1a1a1a; background: transparent; color: #555;
    cursor: pointer; font-family: inherit;
}
.vb-btn:hover { border-color: #333; color: #aaa; }
.vb-btn.leave { border-color: rgba(239,68,68,0.15); color: rgba(239,68,68,0.6); margin-left: auto; }
.vb-btn.leave:hover { background: rgba(239,68,68,0.08); color: #ef4444; }

/* ── User panel ── */
.user-panel {
    background: rgba(10,10,10,0.6); padding: 8px 10px;
    display: flex; align-items: center; gap: 8px;
}
.user-panel .up-name { font-size: 12px; color: #888; }
.user-panel .up-status { font-size: 10px; color: #333; }

/* ── Modals ── */
.modal-backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,0.65);
    display: flex; align-items: center; justify-content: center; z-index: 100;
}
.modal {
    background: #0e0e0e; border: 1px solid #1a1a1a; border-radius: 12px;
    padding: 24px; max-width: 500px; width: 90%;
    box-shadow: 0 16px 48px rgba(0,0,0,0.6);
}
.modal h2 { font-size: 18px; color: #e0e0e0; margin-bottom: 4px; }
.modal .subtitle { font-size: 12px; color: #444; margin-bottom: 16px; }
.modal input {
    width: 100%; padding: 8px 10px; background: #141414;
    border: 1px solid #1a1a1a; border-radius: 6px; color: #bbb;
    font-size: 13px; outline: none; font-family: inherit;
}
.modal input:focus { border-color: #333; }
.modal-buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.btn { padding: 8px 16px; border-radius: 6px; cursor: pointer; font-family: inherit; font-size: 13px; border: none; }
.btn-primary { background: #333; color: #e0e0e0; }
.btn-primary:hover { background: #444; }
.btn-ghost { background: none; color: #777; }
.btn-ghost:hover { background: #1a1a1a; }
.btn-danger { background: rgba(239,68,68,0.1); color: #ef4444; }
.btn-danger:hover { background: rgba(239,68,68,0.2); }

/* ── Welcome ── */
.welcome { padding: 32px 24px 16px; }
.welcome .w-icon { font-size: 24px; color: #222; margin-bottom: 10px; font-family: monospace; }
.welcome h2 { font-size: 20px; font-weight: 600; color: #e0e0e0; margin-bottom: 6px; }
.welcome p { font-size: 12px; color: #333; line-height: 1.6; }
.welcome code { background: #141414; color: #444; padding: 1px 4px; border-radius: 3px; font-size: 11px; }

.date-sep { display: flex; align-items: center; gap: 12px; padding: 6px 24px; }
.date-sep .line { flex: 1; height: 1px; background: #1a1a1a; }
.date-sep .label { font-size: 9px; color: #2a2a2a; font-weight: 500; letter-spacing: 0.5px; }

.typing { font-size: 10px; color: #333; padding: 2px 24px; }

/* ── Search ── */
.search-panel { padding: 12px 8px; }
.search-panel input {
    width: 100%; padding: 6px 8px; background: #141414;
    border: 1px solid #1a1a1a; border-radius: 4px; color: #bbb;
    font-size: 12px; outline: none; font-family: inherit; margin-bottom: 8px;
}
.search-result { padding: 4px 8px; border-radius: 4px; }
.search-result:hover { background: rgba(255,255,255,0.02); }

/* ── Connection badge ── */
.conn-badge {
    display: flex; align-items: center; gap: 4px; font-size: 10px; color: #333;
    background: #141414; padding: 3px 8px; border-radius: 4px;
}

/* ── Settings ── */
.settings-nav { width: 200px; background: rgba(10,10,10,0.5); padding: 16px 0; border-right: 1px solid rgba(255,255,255,0.03); }
.nav-section { font-size: 10px; font-weight: 600; color: #333; letter-spacing: 0.8px; padding: 12px 16px 6px; }
.nav-item {
    display: flex; align-items: center; gap: 8px; padding: 8px 16px;
    font-size: 13px; color: #555; cursor: pointer; border: none; background: none; width: 100%;
}
.nav-item:hover { background: rgba(255,255,255,0.02); color: #aaa; }
.nav-item.active { background: rgba(255,255,255,0.04); color: #fff; border-left: 2px solid #888; }
"#;
