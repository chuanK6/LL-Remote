from pathlib import Path
import shutil, subprocess

workspace = Path(__file__).resolve().parents[2]
extension = Path(__file__).resolve().parent
source = workspace / 'third-party' / 'sources' / 'moonlight'
if not (source / 'app' / 'main.cpp').is_file():
    raise SystemExit('Clone the pinned Moonlight v6.2.0 source first.')

def replace(file, old, new):
    path = source / file
    text = path.read_text(encoding='utf-8')
    if new in text:
        return
    if text.count(old) != 1:
        raise SystemExit(f'Pinned source mismatch: {file}')
    path.write_text(text.replace(old, new, 1), encoding='utf-8', newline='\n')

addon = source / 'app' / 'llremote'
addon.mkdir(parents=True, exist_ok=True)
for name in ['llremotebridge.h', 'llremotebridge.cpp']:
    shutil.copyfile(extension / name, addon / name)
replace('app/app.pro', 'SOURCES += \\\n    backend/nvaddress.cpp', 'SOURCES += llremote/llremotebridge.cpp\nHEADERS += llremote/llremotebridge.h\n\nSOURCES += \\\n    backend/nvaddress.cpp')
replace('app/app.pro', 'QMAKE_LFLAGS += /MANIFEST:embed /MANIFESTINPUT:$${PWD}/Moonlight.exe.manifest', 'QMAKE_LFLAGS += /MANIFEST:embed /MANIFESTINPUT:$$shell_quote($$system_path($$PWD/Moonlight.exe.manifest))')
replace('app/main.cpp', '#include "path.h"', '#include "path.h"\n#include "llremote/llremotebridge.h"')
replace('app/main.cpp', '    SDL_SetMainReady();', '''    bool llSettings = false;
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--ll-settings") == 0) {
            llSettings = true;
            for (int n = i; n < argc; ++n) argv[n] = argv[n + 1];
            --argc; --i;
        }
    }
    SDL_SetMainReady();''')
replace('app/main.cpp', '    if (QFile(QDir::currentPath() + "/portable.dat").exists()) {', '''    const QString llDataPath = QString::fromUtf8(qgetenv("LLREMOTE_MOONLIGHT_DATA_DIR"));
    if (!llDataPath.isEmpty()) {
        QDir().mkpath(llDataPath);
        QSettings::setDefaultFormat(QSettings::IniFormat);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, llDataPath);
        QSettings::setPath(QSettings::IniFormat, QSettings::SystemScope, llDataPath);
        Path::initialize(true);
    }
    else if (QFile(QDir::currentPath() + "/portable.dat").exists()) {''')
replace('app/main.cpp', '    QGuiApplication app(argc, argv);', '    QGuiApplication app(argc, argv);\n    LLRemoteBridge::start();')
replace('app/main.cpp', '        initialView = "qrc:/gui/PcView.qml";', '        initialView = llSettings ? "qrc:/gui/SettingsView.qml" : "qrc:/gui/PcView.qml";')
replace('app/main.cpp', '    bool hasGUI = true;', '    bool hasGUI = true;\n    ComputerManager* llCliManager = nullptr;')
pair_context = '''            auto launcher = new CliPair::Launcher(pairParser.getHost(), pairParser.getPredefinedPin(), &app);
            engine.rootContext()->setContextProperty("launcher", launcher);
            break;'''
managed_pair = '''            auto launcher = new CliPair::Launcher(pairParser.getHost(), pairParser.getPredefinedPin(), &app);
            engine.rootContext()->setContextProperty("launcher", launcher);
            if (qgetenv("LLREMOTE_HEADLESS_PAIR") == "1") {
                hasGUI = false;
                QObject::connect(launcher, &CliPair::Launcher::success, &app, [] { QCoreApplication::exit(0); }, Qt::QueuedConnection);
                QObject::connect(launcher, &CliPair::Launcher::failed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
                llCliManager = new ComputerManager(StreamingPreferences::get());
                launcher->execute(llCliManager);
            }
            break;'''
legacy_pair = managed_pair.replace('                llCliManager = new ComputerManager(StreamingPreferences::get());\n                launcher->execute(llCliManager);', '                launcher->execute(new ComputerManager(StreamingPreferences::get()));')
main_path = source / 'app/main.cpp'
main_text = main_path.read_text(encoding='utf-8')
if legacy_pair in main_text:
    main_path.write_text(main_text.replace(legacy_pair, managed_pair, 1), encoding='utf-8', newline='\n')
replace('app/main.cpp', pair_context, managed_pair)
replace('app/main.cpp', '            auto launcher = new CliListApps::Launcher(listParser.getHost(), listParser, &app);\n            launcher->execute(new ComputerManager(StreamingPreferences::get()));', '            auto launcher = new CliListApps::Launcher(listParser.getHost(), listParser, &app);\n            llCliManager = new ComputerManager(StreamingPreferences::get());\n            launcher->execute(llCliManager);')
replace('app/main.cpp', '    QThreadPool::globalInstance()->waitForDone(30000);', '    QThreadPool::globalInstance()->waitForDone(30000);\n    // Flush the pairing certificate before the CLI process exits.\n    delete llCliManager;\n    llCliManager = nullptr;')
replace('app/backend/computerseeker.cpp', '#include <QTimer>', '#include <QTimer>\n#include <QHostAddress>\n#include <QUrl>')
replace('app/backend/computerseeker.cpp', '    if (!findMatchingComputer()) {', '''    if (qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS")) {
        // Refresh the current address and reuse certificates by Sunshine UUID.
        // Polling saved addresses can otherwise choose a stale route or a namesake.
        m_ComputerManager->addNewHostManually(m_ComputerName);
        return;
    }
    if (!findMatchingComputer()) {''')
replace('app/backend/computerseeker.cpp', '    QString value = m_ComputerName.toLower();', '''    if (qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS")) {
        const QUrl target = QUrl::fromUserInput("moonlight://" + m_ComputerName);
        const QString expectedUuid = qEnvironmentVariable("LLREMOTE_TARGET_UUID");
        const auto active = computer->activeAddress;
        const auto matchesAddress = [&](const QString& host, int port) {
            if (active.port() != port) return false;
            const QHostAddress ip(host);
            return ip.isNull() ? active.address().compare(host, Qt::CaseInsensitive) == 0
                               : QHostAddress(active.address()) == ip;
        };
        if (!expectedUuid.isEmpty() && computer->uuid.compare(expectedUuid, Qt::CaseInsensitive) != 0)
            return false;
        if (QHostAddress(m_ComputerName).protocol() == QAbstractSocket::IPv6Protocol)
            return matchesAddress(m_ComputerName, DEFAULT_HTTP_PORT);
        return target.isValid() && !target.host().isEmpty()
            && matchesAddress(target.host(), target.port(DEFAULT_HTTP_PORT));
    }

    QString value = m_ComputerName.toLower();''')
replace('app/backend/computerseeker.cpp', '#include "computermanager.h"', '#include "computermanager.h"\n#include "nvhttp.h"')
replace('app/backend/computerseeker.cpp', '    NvComputer* matchingComputer = findMatchingComputer();', '    NvComputer* matchingComputer = qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS") ? nullptr : findMatchingComputer();')
replace('app/backend/computerseeker.cpp', '        m_ComputerManager->stopPollingAsync();\n        m_TimeoutTimer->stop();', '''        if (!qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS")) {
            m_ComputerManager->stopPollingAsync();
        }
        m_TimeoutTimer->stop();''')
replace('app/backend/computerseeker.cpp', '    m_TimeoutTimer->stop();\n    m_ComputerManager->stopPollingAsync();\n    emit errorTimeout();', '''    m_TimeoutTimer->stop();
    if (!qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS")) {
        m_ComputerManager->stopPollingAsync();
    }
    emit errorTimeout();''')
replace('app/backend/computermanager.cpp', '        NvComputer* newComputer = new NvComputer(http, serverInfo);', '''        NvComputer* newComputer = new NvComputer(http, serverInfo);
        const bool exactAddress = !m_Mdns && qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS");
        const QString expectedUuid = qEnvironmentVariable("LLREMOTE_TARGET_UUID");
        if (exactAddress && !expectedUuid.isEmpty() && newComputer->uuid.compare(expectedUuid, Qt::CaseInsensitive) != 0) {
            delete newComputer;
            emit computerAddCompleted(false, false);
            return;
        }''')
replace('app/backend/computermanager.cpp', '                if (changed) {\n                    qInfo() << existingComputer->name', '                if (changed || exactAddress) {\n                    qInfo() << existingComputer->name')
replace('app/backend/computermanager.cpp', '                if (!m_Mdns && addressIsSiteLocalV4 && newComputer->getActiveAddressReachability() != NvComputer::RI_VPN) {', '                if (!m_Mdns && !exactAddress && addressIsSiteLocalV4 && newComputer->getActiveAddressReachability() != NvComputer::RI_VPN) {')
replace('app/backend/computermanager.cpp', '                if (m_ComputerManager->m_Prefs->detectNetworkBlocking) {', '                if (!qEnvironmentVariableIsSet("LLREMOTE_EXACT_ADDRESS") && m_ComputerManager->m_Prefs->detectNetworkBlocking) {')
replace('app/backend/computermanager.cpp', '            NvComputer httpsComputer(http, serverInfo);\n            newComputer->update(httpsComputer);', '''            NvComputer httpsComputer(http, serverInfo);
            if (exactAddress && httpsComputer.uuid != newComputer->uuid) {
                delete newComputer;
                emit computerAddCompleted(false, false);
                return;
            }
            newComputer->update(httpsComputer);''')
replace('app/backend/computermanager.cpp', '        // Update addresses depending on the context', '''        if (exactAddress && qgetenv("LLREMOTE_EXACT_ADDRESS") == "stream" &&
            newComputer->pairState == NvComputer::PS_PAIRED) {
            // Streaming normally receives an app list from the polling thread.
            // Fetch it from the verified address before notifying the launcher.
            try {
                newComputer->appList = http.getAppList();
            } catch (const std::exception&) {
                delete newComputer;
                emit computerAddCompleted(false, false);
                return;
            }
        }

        // Update addresses depending on the context''')
replace('app/main.cpp', 'commandLineParserResult == GlobalCommandLineParser::NormalStartRequested);', 'commandLineParserResult == GlobalCommandLineParser::NormalStartRequested && !llSettings);')
replace('app/main.cpp', '        if (engine.rootObjects().isEmpty())\n            return -1;', '        if (engine.rootObjects().isEmpty())\n            return -1;\n        LLRemoteBridge::setSettingsVisible(llSettings);')
replace('app/path.cpp', '        s_LogDir = QDir::currentPath();', '        const QString llBase = qgetenv("LLREMOTE_MOONLIGHT_DATA_DIR").isEmpty() ? QDir::currentPath() : QString::fromUtf8(qgetenv("LLREMOTE_MOONLIGHT_DATA_DIR"));\n        s_LogDir = llBase;')
for leaf in ['boxart', 'qmlcache', 'cache']:
    replace('app/path.cpp', f'QDir::currentPath() + "/{leaf}"', f'llBase + "/{leaf}"')
replace('app/streaming/session.cpp', '#include "session.h"', '#include "session.h"\n#include "../llremote/llremotebridge.h"\n#include <QJsonObject>\n#include <memory>')
replace('app/streaming/session.cpp', '    emit connectionStarted();', '    LLRemoteBridge::setConnected(true);\n    emit connectionStarted();')
event_start = '        case SDL_USEREVENT:\n            switch (event.user.code) {'
old_event = event_start + '''
            case LLRemoteBridge::EventCode: {
                std::unique_ptr<QJsonObject> request(static_cast<QJsonObject*>(event.user.data1));
                llRemoteRequest(*request);
                break;
            }'''
new_event = event_start + '''
            case LLRemoteBridge::EventCode:
                LLRemoteBridge::processRequest([this](const QJsonObject& request) {
                    return llRemoteRequest(request);
                });
                if (m_ShouldExit) goto DispatchDeferredCleanup;
                break;'''
session_source = source / 'app/streaming/session.cpp'
session_text = session_source.read_text(encoding='utf-8')
if old_event in session_text:
    session_source.write_text(session_text.replace(old_event, new_event, 1), encoding='utf-8', newline='\n')
replace('app/streaming/session.cpp', event_start, new_event)
replace('app/streaming/session.cpp', '    m_InputHandler->setCaptureActive(false);', '    LLRemoteBridge::setConnected(false);\n    m_InputHandler->setCaptureActive(false);')
header = source / 'app/streaming/session.h'
header_text = header.read_text(encoding='utf-8')
old_declaration = '    void llRemoteRequest(const QJsonObject& request);\n'
if old_declaration in header_text:
    header.write_text(header_text.replace(old_declaration, '', 1), encoding='utf-8', newline='\n')
replace('app/streaming/session.h', '    void exec();', '    bool llRemoteRequest(const QJsonObject& request);\n    bool setFullscreenMode(Uint32 mode);\n    void exec();')
replace('app/streaming/session.h', 'class Session : public QObject', 'class QJsonObject;\n\nclass Session : public QObject')
replace('app/streaming/session.cpp', '''    bool fullScreen = !(SDL_GetWindowFlags(m_Window) & m_FullScreenFlag);
''', '''    const bool fullScreen = (SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN) != 0;
    setFullscreenMode(fullScreen ? 0 : m_FullScreenFlag);
}

bool Session::setFullscreenMode(Uint32 mode)
{
    const auto current = SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN_DESKTOP;
    if (current == mode) return true;

    int oldWidth, oldHeight;
    SDL_GetWindowSize(m_Window, &oldWidth, &oldHeight);
''')
replace('app/streaming/session.cpp', '    SDL_SetWindowFullscreen(m_Window, fullScreen ? m_FullScreenFlag : 0);', '''    const bool applied = SDL_SetWindowFullscreen(m_Window, mode) == 0;
    if (!applied) {
        SDL_LogWarn(SDL_LOG_CATEGORY_APPLICATION, "Failed to change window mode: %s", SDL_GetError());
    }''')
replace('app/streaming/session.cpp', '    if (!fullScreen && m_FullScreenFlag == SDL_WINDOW_FULLSCREEN) {', '    if (applied && mode == 0 && current == SDL_WINDOW_FULLSCREEN) {')
replace('app/streaming/session.cpp', '''    m_InputHandler->updatePointerRegionLock();
}

void Session::notifyMouseEmulationMode''', '''    m_InputHandler->updatePointerRegionLock();

    // Even a same-size mode change needs a replacement decoder after the reset above.
    int width, height;
    SDL_GetWindowSize(m_Window, &width, &height);
    if (width == oldWidth && height == oldHeight) {
        SDL_Event resize{};
        resize.type = SDL_WINDOWEVENT;
        resize.window.windowID = SDL_GetWindowID(m_Window);
        resize.window.event = SDL_WINDOWEVENT_SIZE_CHANGED;
        resize.window.data1 = width;
        resize.window.data2 = height;
        SDL_PushEvent(&resize);
    }
    return applied;
}

void Session::notifyMouseEmulationMode''')
replace('app/streaming/session.cpp', '    Uint32 defaultWindowFlags = SDL_WINDOW_ALLOW_HIGHDPI | SDL_WINDOW_RESIZABLE;', '''    if (!qgetenv("LLREMOTE_PIPE_NAME").isEmpty()) {
        SDL_SetHint(SDL_HINT_VIDEO_MINIMIZE_ON_FOCUS_LOSS, "0");
    }
    Uint32 defaultWindowFlags = SDL_WINDOW_ALLOW_HIGHDPI | SDL_WINDOW_RESIZABLE;''')
text = session_source.read_text(encoding='utf-8')
for declaration in ('\nvoid Session::llRemoteRequest(', '\nbool Session::llRemoteRequest('):
    if declaration in text:
        text = text.split(declaration, 1)[0].rstrip() + '\n'
session_source.write_text(text + '\n' + (extension / 'session-management.cpp.inc').read_text(encoding='utf-8'), encoding='utf-8', newline='\n')
replace('app/streaming/session.cpp', '    // Input handler might need to start/stop keyboard grab after changing modes', '''    if (applied && mode == 0 && !qgetenv("LLREMOTE_PIPE_NAME").isEmpty()) {
        m_InputHandler->raiseAllKeys();
        m_InputHandler->setCaptureActive(false);
        SDL_SetWindowBordered(m_Window, SDL_TRUE);
        SDL_SetWindowResizable(m_Window, SDL_TRUE);
    }

    // Input handler might need to start/stop keyboard grab after changing modes''')
replace('app/streaming/input/input.cpp', '    if (!(SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN) && !m_AbsoluteMouseMode) {', '''    if (!(SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN) &&
        (!m_AbsoluteMouseMode || !qgetenv("LLREMOTE_PIPE_NAME").isEmpty())) {
        SDL_CaptureMouse(SDL_FALSE);
        m_PendingMouseButtonsAllUpOnVideoRegionLeave = false;''')
replace('app/streaming/input/input.cpp', '''        if (m_AbsoluteMouseMode || SDL_SetRelativeMouseMode(SDL_TRUE) < 0) {
            // Relative mouse mode didn't work or was disabled, so we'll just hide the cursor
            SDL_ShowCursor(m_MouseCursorCapturedVisibilityState);''', '''        const bool desktopWindow = !qgetenv("LLREMOTE_PIPE_NAME").isEmpty() &&
                                   !(SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN);
        if (m_AbsoluteMouseMode || desktopWindow || SDL_SetRelativeMouseMode(SDL_TRUE) < 0) {
            // In a desktop window, send relative deltas without trapping the local pointer.
            SDL_ShowCursor(desktopWindow && !m_AbsoluteMouseMode
                           ? SDL_ENABLE : m_MouseCursorCapturedVisibilityState);''')
replace('app/streaming/input/mouse.cpp', '    if (isCaptureActive() && m_PointerRegionLockActive) {', '''    const bool desktopWindow = !qgetenv("LLREMOTE_PIPE_NAME").isEmpty() &&
                               !(SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN);
    if (isCaptureActive() && m_PointerRegionLockActive && !desktopWindow) {''')
replace('app/streaming/input/input.h', '    void setCaptureActive(bool active);', '''    void setCaptureActive(bool active);
    void llRemoteSetMouseMode(bool absolute) {
        const bool captured = isCaptureActive();
        setCaptureActive(false);
        m_AbsoluteMouseMode = absolute;
        if (captured) setCaptureActive(true);
        updatePointerRegionLock();
    }''')
replace('app/streaming/input/input.h', '#include "SDL_compat.h"', '#include "SDL_compat.h"\n\nclass QJsonObject;')
replace('app/streaming/input/input.h', '    bool isCaptureActive();', '    bool isCaptureActive();\n    void llRemoteSetShortcuts(const QJsonObject& shortcuts);')
replace('app/streaming/input/input.h', '    void performSpecialKeyCombo(KeyCombo combo);', '''    bool llRemoteHandleShortcut(const SDL_KeyboardEvent* event);
    struct LlRemoteShortcut {
        SDL_Keycode key;
        Uint16 modifiers;
    } m_LlRemoteShortcuts[3]{};
    SDL_Scancode m_LlRemoteShortcutRelease = SDL_SCANCODE_UNKNOWN;
    void performSpecialKeyCombo(KeyCombo combo);''')
replace('app/streaming/input/input.cpp', '#include <QGuiApplication>', '#include <QGuiApplication>\n#include <QJsonDocument>\n#include <QJsonObject>')
replace('app/streaming/input/input.cpp', '    m_SpecialKeyCombos[KeyComboToggleKeyboardGrab].enabled = WMUtils::isRunningDesktopEnvironment();', '''    m_SpecialKeyCombos[KeyComboToggleKeyboardGrab].enabled = WMUtils::isRunningDesktopEnvironment();
    if (qEnvironmentVariableIsSet("LLREMOTE_SHORTCUTS")) {
        llRemoteSetShortcuts(QJsonDocument::fromJson(qgetenv("LLREMOTE_SHORTCUTS")).object());
    }''')
replace('app/streaming/input/keyboard.cpp', '#include "SDL_compat.h"', '#include "SDL_compat.h"\n#include <QJsonObject>\n#include <cstdio>')
replace('app/streaming/input/keyboard.cpp', 'void SdlInputHandler::handleKeyEvent(SDL_KeyboardEvent* event)', '''void SdlInputHandler::llRemoteSetShortcuts(const QJsonObject& shortcuts)
{
    const char* actions[] = {"mouse", "disconnect", "display"};
    for (int i = 0; i < 3; ++i) {
        const auto binding = shortcuts[actions[i]].toObject();
        const auto key = binding["key"].toString().toLatin1();
        m_LlRemoteShortcuts[i] = {SDL_GetKeyFromName(key.constData()), static_cast<Uint16>(
            (binding["ctrl"].toBool() ? 1 : 0) |
            (binding["alt"].toBool() ? 2 : 0) |
            (binding["shift"].toBool() ? 4 : 0))};
    }
    m_SpecialKeyCombos[KeyComboUngrabInput].enabled = false;
    m_SpecialKeyCombos[KeyComboToggleMinimize].enabled = false;
    m_SpecialKeyCombos[KeyComboTogglePointerRegionLock].enabled = false;
}

bool SdlInputHandler::llRemoteHandleShortcut(const SDL_KeyboardEvent* event)
{
    if (event->state == SDL_RELEASED && m_LlRemoteShortcutRelease != SDL_SCANCODE_UNKNOWN &&
        event->keysym.scancode == m_LlRemoteShortcutRelease) {
        m_LlRemoteShortcutRelease = SDL_SCANCODE_UNKNOWN;
        return true;
    }
    if (event->state != SDL_PRESSED || (event->keysym.mod & KMOD_GUI)) return false;
    const auto mods = event->keysym.mod;
    const Uint16 modifierMask = (mods & KMOD_CTRL ? 1 : 0) |
                               (mods & KMOD_ALT ? 2 : 0) |
                               (mods & KMOD_SHIFT ? 4 : 0);
    for (int i = 0; i < 3; ++i) {
        const auto& binding = m_LlRemoteShortcuts[i];
        if (binding.key == SDLK_UNKNOWN || binding.modifiers != modifierMask ||
            (event->keysym.sym != binding.key && event->keysym.scancode != SDL_GetScancodeFromKey(binding.key))) continue;

        m_LlRemoteShortcutRelease = event->keysym.scancode;
        if (i == 0) {
            performSpecialKeyCombo(KeyComboUngrabInput);
        } else {
            raiseAllKeys();
            fputs(i == 1 ? "LLREMOTE_SHORTCUT_DISCONNECT\\n" : "LLREMOTE_SHORTCUT_DISPLAY\\n", stdout);
            fflush(stdout);
        }
        return true;
    }
    return false;
}

void SdlInputHandler::handleKeyEvent(SDL_KeyboardEvent* event)''')
replace('app/streaming/input/keyboard.cpp', '    // Check for our special key combos', '''    if (llRemoteHandleShortcut(event)) return;

    // Check for our special key combos''')
replace('app/streaming/video/ffmpeg.cpp', '#include "ffmpeg.h"', '#include "ffmpeg.h"\n#include "../../llremote/llremotebridge.h"')
replace('app/streaming/video/ffmpeg.cpp', '        if (Session::get()->getOverlayManager().isOverlayEnabled(Overlay::OverlayDebug)) {', '        if (Session::get()->getOverlayManager().isOverlayEnabled(Overlay::OverlayDebug) || !qgetenv("LLREMOTE_PIPE_NAME").isEmpty()) {')
replace('app/streaming/video/ffmpeg.cpp', '            stringifyVideoStats(lastTwoWndStats,', '            LLRemoteBridge::publishMetrics(lastTwoWndStats);\n            stringifyVideoStats(lastTwoWndStats,')
replace('app/streaming/audio/renderers/sdlaud.cpp', '#include "sdl.h"', '#include "sdl.h"\n#include <QByteArray>')
replace('app/streaming/audio/renderers/sdlaud.cpp', '    m_AudioDevice = SDL_OpenAudioDevice(NULL, 0, &want, &have, 0);', '    const auto llDevice = qgetenv("LLREMOTE_AUDIO_DEVICE");\n    m_AudioDevice = SDL_OpenAudioDevice(llDevice.isEmpty() ? NULL : llDevice.constData(), 0, &want, &have, 0);')
patch = subprocess.check_output(['git', '-C', str(source), 'diff', '--binary'], text=True, encoding='utf-8')
(extension / 'moonlight-v6.2.0.patch').write_text(patch, encoding='utf-8', newline='\n')
print('Applied LLRemote management extension to pinned Moonlight source.')
