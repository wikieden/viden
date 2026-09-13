/* ════════════════════════════════════════════════════════════════
 * gui-icons.jsx · Viden GUI 图标（canonical · 单一真源）
 *
 * 所有 GUI 桌面屏共用的线性图标 + agent 品牌徽标。视觉母版 = D1 驾驶舱。
 * 抽成一份后：活动 rail / 标题栏工具组 / 视图切换各页零重画，改一处全改。
 * 此前 D1 内联 I* 一套、D2/D7/D12/D13 各自 VRAIL_ICONS 一套、标题栏 I 一套——
 * 同一个 worktree/lanes/review 被各画各的。本文件把它们收敛成一套权威画法。
 *
 * 接入（页面已加载 React + Babel + gui-kit.css）：
 *   <script type="text/babel" src="../gui-icons.jsx"></script>   // gui-titlebar / 主脚本之前
 *
 * 用法：
 *   {ICONS.chat}                          // 直接拿元素（rail / 标题栏按钮内）
 *   <GuiIcon name="lock" className="ic sm"/>  // 换 class / 尺寸（小号 15px）
 *   <GuiIcon name="lock" style={{width:13,height:13}}/>  // 任意尺寸（URL 栏等）
 *   <AgentLogo agent="codex"/>            // agent 品牌徽标（17×17，品牌色非主题 token）
 *
 * 画法规范：线性 · viewBox 0 0 24 24 · class "ic"（gui-kit svg.ic 给 stroke
 * currentColor / width 1.7 / 圆角端点）；"ic sm" = 15px。term 沿用其 16-grid 原画。
 * 颜色一律 currentColor（继承父级），唯独 AgentLogo 用品牌固定色（codex 绿 / claude
 * 橙 / kiro 紫 / viden 青+金），刻意不随主题换肤——这是各家 agent 的身份色。
 *
 * 登记：新图标先在此处加 key + 在 DESIGN-REF「GUI 图标目录」登记语义，再在页面引用。
 *       没登记 = 临时草稿。GUI 内禁 emoji，要图标一律走这里的线性 SVG。
 * ════════════════════════════════════════════════════════════════ */
(function () {
  const S = (children, vb) => (
    <svg className="ic" viewBox={vb || '0 0 24 24'}>{children}</svg>
  );

  const ICONS = {
    /* ── 活动 rail / 视图 ── */
    chat: S(<path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7A8.38 8.38 0 0 1 4 11.5 8.5 8.5 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z"/>),
    worktree: S(<g><circle cx="6" cy="6" r="2.4"/><circle cx="6" cy="18" r="2.4"/><circle cx="18" cy="9" r="2.4"/><path d="M6 8.4v7.2M18 11.4c0 3-3 3.6-5.4 3.6H8.4"/></g>),
    lanes: S(<g><path d="M4 5v14M12 5v14M20 5v14" strokeDasharray="3 3"/><circle cx="4" cy="10" r="2.2"/><circle cx="12" cy="14" r="2.2"/><circle cx="20" cy="8" r="2.2"/></g>),
    review: S(<path d="M9 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h4M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4M12 4v16"/>),
    decide: S(<path d="M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>),
    /* plan = 步骤清单（首项已勾、其余待办）—— D15 Plan Studio 的 rail 目的地。
       刻意不画成第二个剪贴板：decide 已占「剪贴板 + 勾」，本图形讲的是 steps 本身。 */
    plan: S(<g><path d="M3.2 6.4l1.7 1.7L8.4 4.6"/><path d="M3.4 12.6h2.2M3.4 18.6h2.2"/><path d="M11 6.4h9.6M11 12.6h9.6M11 18.6h6.2"/></g>),
    fleet: S(<g><circle cx="12" cy="5" r="2.4"/><circle cx="5" cy="19" r="2.2"/><circle cx="12" cy="19" r="2.2"/><circle cx="19" cy="19" r="2.2"/><path d="M12 7.4v4M12 11.4L5.6 16.8M12 11.4l6.4 5.4M12 11.4V17"/></g>),
    evidence: S(<g><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><path d="M9 13h6M9 17h4"/></g>),
    diagnostics: S(<path d="M3 12h4l3 8 4-16 3 8h4"/>),
    inbox: S(<path d="M22 12h-6l-2 3h-4l-2-3H2M5 5h14l3 7v6a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-6z"/>),
    brief: S(<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M9 13h6M9 17h6"/>),
    gallery: S(<g><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="M21 15l-5-5L5 21"/></g>),
    remote: S(<g><path d="M5 12h14M12 5l7 7-7 7"/><circle cx="4" cy="12" r="1.5"/></g>),
    rocket: S(<path d="M4.5 16.5c-1.5 1.3-2 5-2 5s3.7-.5 5-2c.7-.8.7-2 0-2.7a1.9 1.9 0 0 0-3 0zM12 15l-3-3a12 12 0 0 1 8-9 12 12 0 0 1 1 8 12 12 0 0 1-6 4z"/>),

    /* ── 标题栏工具组 ── */
    palette: S(<path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z"/>),
    focus: S(<path d="M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3"/>),
    popout: S(<path d="M15 3h6v6M10 14L21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>),
    term: <svg className="ic" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3"><rect x="1.5" y="2.5" width="13" height="11" rx="1.5"/><rect x="2.5" y="9.6" width="11" height="3.4" rx="1" fill="currentColor" stroke="none" opacity="0.55"/><line x1="1.5" y1="9.5" x2="14.5" y2="9.5"/></svg>,
    panel: S(<g><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/></g>),
    slot: S(<circle cx="12" cy="12" r="6.5" strokeDasharray="3 3"/>),
    pin: S(<path d="M9 3h6M10 3v6l-3 3v2h10v-2l-3-3V3M12 16v5"/>),
    settings: S(<g><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></g>),

    /* ── 替代 emoji 的线性图标（GUI 禁 emoji）── */
    lock: S(<g><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/><circle cx="12" cy="15.3" r="1.15"/></g>),
    robot: S(<g><rect x="5" y="8" width="14" height="11" rx="2.5"/><path d="M12 4v4M9.5 13h0M14.5 13h0M3.5 12v3M20.5 12v3"/><circle cx="9.5" cy="13" r="1.2"/><circle cx="14.5" cy="13" r="1.2"/><path d="M9.5 16.5h5"/></g>),
    game: S(<g><rect x="2.5" y="7.5" width="19" height="10" rx="5"/><path d="M7.5 11v3M6 12.5h3"/><circle cx="15.5" cy="11.8" r="1"/><circle cx="18" cy="14" r="1"/></g>),
    ml: S(<path d="M3 17l5-5 4 4 8-9M16 7h5v5"/>),
  };

  function GuiIcon(props) {
    const el = ICONS[props.name];
    if (!el) return null;
    const next = {};
    if (props.className != null) next.className = props.className;
    if (props.style != null) next.style = props.style;
    return React.cloneElement(el, next);
  }

  /* ── agent 品牌徽标（17×17 · 品牌固定色，刻意不随主题换肤）── */
  function AgentLogo({ agent }) {
    const a = agent;
    if (a === 'codex') {
      return <svg viewBox="0 0 24 24" style={{ color: '#10a37f' }}><path fill="currentColor" d="M21.55 10.004a5.42 5.42 0 0 0-.478-4.501c-1.217-2.09-3.662-3.166-6.05-2.66A5.59 5.59 0 0 0 10.831 1C8.39.995 6.224 2.546 5.473 4.838A5.55 5.55 0 0 0 1.76 7.496a5.49 5.49 0 0 0 .691 6.5 5.42 5.42 0 0 0 .477 4.502c1.217 2.09 3.662 3.165 6.05 2.66A5.59 5.59 0 0 0 13.168 23c2.443.006 4.61-1.546 5.361-3.84a5.55 5.55 0 0 0 3.715-2.66 5.49 5.49 0 0 0-.693-6.497zm-8.381 11.558a4.2 4.2 0 0 1-2.675-.954l.132-.074 4.44-2.53a.71.71 0 0 0 .364-.623v-6.176l1.877 1.069a.06.06 0 0 1 .036.052v5.115c0 2.274-1.84 4.118-4.111 4.12zM4.292 17.66a4.07 4.07 0 0 1-.49-2.749l.132.078 4.44 2.53a.72.72 0 0 0 .72 0l5.42-3.088v2.138a.06.06 0 0 1-.024.052L9.69 21.176c-1.97 1.135-4.487.46-5.622-1.51zM3.122 8.108a4.1 4.1 0 0 1 2.142-1.803V11.5a.71.71 0 0 0 .357.62l5.42 3.088-1.877 1.069a.07.07 0 0 1-.062.005l-4.489-2.559a4.12 4.12 0 0 1-1.491-5.615zm15.417 3.587-5.42-3.088 1.877-1.068a.07.07 0 0 1 .062-.005l4.489 2.559a4.116 4.116 0 0 1-.621 7.42v-5.198a.71.71 0 0 0-.367-.62zm1.867-2.811a6.4 6.4 0 0 0-.132-.078l-4.44-2.53a.72.72 0 0 0-.72 0L9.71 9.364V7.227a.06.06 0 0 1 .024-.053l4.49-2.559a4.115 4.115 0 0 1 6.112 4.27zM8.69 12.731l-1.877-1.069a.06.06 0 0 1-.036-.052V6.495c0-2.276 1.846-4.121 4.121-4.12a4.2 4.2 0 0 1 2.674.954l-.132.074-4.44 2.53a.71.71 0 0 0-.364.623zm1.02-2.198 2.415-1.376 2.416 1.376v2.752l-2.415 1.376-2.416-1.376z"/></svg>;
    }
    if (a === 'claude') {
      const rays = [];
      for (let i = 0; i < 13; i++) {
        const ang = (i / 13) * Math.PI * 2 - Math.PI / 2;
        const x1 = 12 + Math.cos(ang) * 3.2, y1 = 12 + Math.sin(ang) * 3.2;
        const x2 = 12 + Math.cos(ang) * 10.5, y2 = 12 + Math.sin(ang) * 10.5;
        rays.push(<line key={i} x1={x1} y1={y1} x2={x2} y2={y2} stroke="#d97757" strokeWidth="2.1" strokeLinecap="round"/>);
      }
      return <svg viewBox="0 0 24 24">{rays}</svg>;
    }
    if (a === 'kiro') {
      return <svg viewBox="0 0 24 24"><path fill="#8b5cf6" d="M4 11a8 8 0 0 1 16 0v8.4c0 .8-.94 1.2-1.52.66l-1.3-1.2a1 1 0 0 0-1.36 0l-1.18 1.1a1 1 0 0 1-1.36 0l-1.12-1.04a1 1 0 0 0-1.36 0l-1.12 1.04a1 1 0 0 1-1.36 0l-1.18-1.1a1 1 0 0 0-1.36 0l-1.3 1.2C4.94 20.6 4 20.2 4 19.4z"/><ellipse cx="9.2" cy="10.5" rx="1.5" ry="2.1" fill="var(--bg-panel)"/><ellipse cx="14.8" cy="10.5" rx="1.5" ry="2.1" fill="var(--bg-panel)"/></svg>;
    }
    // viden — bracket + eye, brand mark
    return <svg viewBox="0 0 92 92"><path d="M22 18 H12 V74 H22" fill="none" stroke="#34bdd9" strokeWidth="6"/><path d="M70 18 H80 V74 H70" fill="none" stroke="#34bdd9" strokeWidth="6"/><circle cx="46" cy="46" r="17" fill="none" stroke="#74e4f2" strokeWidth="5.5"/><circle cx="46" cy="46" r="6.5" fill="#e3ab44"/></svg>;
  }

  window.ICONS = ICONS;
  window.GuiIcon = GuiIcon;
  window.AgentLogo = AgentLogo;
})();
