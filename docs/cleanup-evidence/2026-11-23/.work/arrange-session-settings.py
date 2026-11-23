from pathlib import Path
path = Path('src/pages/SessionPanel.vue')
text = path.read_text(encoding='utf-8')
changes = [
    ('</button></div><div class="float-label">画质</div>', '</button></div>\n    <section class="session-group"><h3>画质</h3>'),
    ('保留远端应用。</p>', '保留远端应用。</p></section>'),
    ('<div class="float-label">鼠标控制</div>', '<section class="session-group"><h3>鼠标控制</h3>'),
    ('</button></div><div class="float-label">窗口模式</div>', '</button></div></section>\n    <section class="session-group"><h3>窗口模式</h3>'),
    ('</button></div><div class="float-label">音频设置</div>', '</button></div></section>\n    <section class="session-group"><h3>音频</h3>'),
    ('placeholder="系统默认设备"/></div>', 'placeholder="系统默认设备"/></div></section>'),
    ('    <div class="monitor-row">', '    <section class="session-group"><h3>性能</h3><div class="monitor-row">'),
    ('</dd></dl><FeedbackMessage', '</dd></dl></section><FeedbackMessage'),
    ('  inset: 4px;\n}\n</style>', '''  inset: 4px;
}
.session-group + .session-group { margin-top: 15px; }
.session-group h3 {
  margin: 0 0 6px;
  color: var(--ink);
  font-size: 13px;
  font-weight: 600;
}
.session-group .monitor-row {
  margin-top: 0;
  padding-block: 4px;
  border-top: 0;
}
.disconnect-button { margin-top: 14px; }
</style>'''),
]
for before, after in changes:
    if before not in text: raise ValueError(before)
    text = text.replace(before, after, 1)
path.write_text(text, encoding='utf-8', newline='\n')

