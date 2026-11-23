#pragma once
#include <QJsonObject>
#include <functional>
struct _VIDEO_STATS;

// Management extension only. The existing protocol, decoder and input engine remain intact.
class LLRemoteBridge {
public:
    static constexpr int EventCode = 0x4c4c;
    static void start();
    static void setConnected(bool connected);
    static void setSettingsVisible(bool visible);
    static void processRequest(const std::function<bool(const QJsonObject&)>& apply);
    static void publishMetrics(const _VIDEO_STATS& stats);
    static bool applyAudio(int volume, bool muted);
};
