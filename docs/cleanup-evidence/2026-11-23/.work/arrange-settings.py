from pathlib import Path
pages = Path('src/pages')
def edit(name, replacements):
    path = pages / f'{name}.vue'
    text = path.read_text(encoding='utf-8')
    for before, after in replacements:
        if before not in text: raise ValueError(before)
        text = text.replace(before, after, 1)
    path.write_text(text, encoding='utf-8', newline='\n')
edit('AppsPage', [
    ('<template v-if="application === \'moonlight\'">', '<template v-if="application === \'moonlight\'">'),
    ('设备使用的默认画质、窗口、输入和声音。\'"/><section', '设备使用的默认画质、窗口、输入和声音。\'"/>\n    <div class="settings-groups">\n    <section'),
    ('<section id="inputSection" class="page-section"><h2>输入与声音</h2>', '<section id="inputSection" class="page-section"><h2>鼠标、键盘与手柄</h2>'),
    ('</SettingRow><SettingRow label="音频声道">', '</SettingRow></section>\n    <section class="page-section"><h2>声音与输出</h2><SettingRow label="音频声道">'),
    ('</section><FeedbackMessage v-if="app.snapshot.session.stage', '</section>\n    </div>\n    <FeedbackMessage v-if="app.snapshot.session.stage'),
    ('管理本机采集服务、管理账号与新设备配对。"/><section', '管理本机采集服务、管理账号与新设备配对。"/>\n    <div class="settings-groups">\n    <section'),
    ('</SettingRow><SettingRow label="已发布应用">', '</SettingRow></section>\n    <section class="page-section"><h2>远程应用</h2><SettingRow label="已发布应用">'),
    ('</section><FeedbackMessage message="采集配置', '</section>\n    </div>\n    <FeedbackMessage message="采集配置'),
    ('使用皎月连账号与组网连接。"/><section class="page-section"><h2>账号与网络</h2>', '使用皎月连账号与组网连接。"/>\n    <div class="settings-groups">\n    <section class="page-section"><h2>账号</h2>'),
    ('</SettingRow><SettingRow label="主要连接方式">', '</SettingRow><SettingRow label="本机名称"><span class="value-text">{{ app.snapshot.account.localName || \'登录后读取\' }}</span></SettingRow></section>\n    <section class="page-section"><h2>组网信息</h2><SettingRow label="主要连接方式">'),
    ('访问码 + 密码连接 →</button></SettingRow></section>', '识别码、密码与组网地址连接 →</button></SettingRow></section>\n    </div>'),
])
edit('AccountPage', [
    ('可读取的连接时长。"/><section', '可读取的连接时长。"/><div class="settings-groups"><section'),
    ('</SettingRow></section><div class="page-actions">', '</SettingRow></section></div><div class="page-actions">'),
])
edit('QuickAppsPage', [
    ('<div v-if="device" class="page-section">', '<div v-if="device" class="settings-groups"><section class="page-section">'),
    ('</button></div></div></template>', '</button></div></section></div></template>'),
])

