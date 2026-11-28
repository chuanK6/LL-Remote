bool Session::llRemoteRequest(const QJsonObject& request)
{
    const auto action = request["action"].toString();
    const auto payload = request["payload"].toObject();
    if (action == "disconnect") {
        setShouldExit(false);
        Q_ASSERT(m_ShouldExit);
        return true;
    }
    if (action == "input") {
        m_InputHandler->raiseAllKeys();
        m_InputHandler->setCaptureActive(payload["toggle"].toBool() && !m_InputHandler->isCaptureActive());
        return true;
    }
    if (action != "apply") return false;
    const auto patch = payload["patch"].toObject();
    const auto preferences = payload["preferences"].toObject();
    const auto kind = patch["kind"].toString();
    if (kind == "shortcuts") {
        m_InputHandler->llRemoteSetShortcuts(payload["shortcuts"].toObject());
        return true;
    }
    if (kind == "mouse") {
        const bool absolute = preferences["mouseMode"].toString() == "office";
        m_Preferences->absoluteMouseMode = absolute;
        m_InputHandler->raiseAllKeys();
        m_InputHandler->setCaptureActive(false);
        m_InputHandler->llRemoteSetMouseMode(absolute);
        return true;
    }
    else if (kind == "window" && m_Window) {
        const auto mode = preferences["windowMode"].toString();
        const Uint32 target = mode == "windowed" ? 0 :
            mode == "fullscreen" ? SDL_WINDOW_FULLSCREEN : SDL_WINDOW_FULLSCREEN_DESKTOP;
        const auto current = SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN_DESKTOP;
        if (current != target) {
            // Release the exclusive display mode before entering desktop fullscreen.
            if (current != 0 && !setFullscreenMode(0)) return false;
            if (target != 0 && !setFullscreenMode(target)) return false;
        }
        if ((SDL_GetWindowFlags(m_Window) & SDL_WINDOW_FULLSCREEN_DESKTOP) != target) return false;
        if (target == 0) {
            SDL_Rect usable{};
            if (SDL_GetDisplayUsableBounds(SDL_GetWindowDisplayIndex(m_Window), &usable) == 0) {
                int width, height;
                SDL_GetWindowSize(m_Window, &width, &height);
                // Some Windows drivers restore a maximized or monitor-sized window.
                if ((SDL_GetWindowFlags(m_Window) & SDL_WINDOW_MAXIMIZED) ||
                    width >= usable.w || height >= usable.h) {
                    SDL_RestoreWindow(m_Window);
                    SDL_Rect source{0, 0, m_StreamConfig.width, m_StreamConfig.height};
                    SDL_Rect size{0, 0, usable.w * 4 / 5, usable.h * 4 / 5};
                    StreamUtils::scaleSourceToDestinationSurface(&source, &size);
                    SDL_SetWindowSize(m_Window, size.w, size.h);
                    SDL_SetWindowPosition(m_Window, usable.x + (usable.w - size.w) / 2,
                                          usable.y + (usable.h - size.h) / 2);
                }
            }
        } else {
            m_FullScreenFlag = target;
        }
        return true;
    }
    else if (kind == "audio") {
        return LLRemoteBridge::applyAudio(preferences["volume"].toInt(75), preferences["muted"].toBool());
    }
    return false;
}
