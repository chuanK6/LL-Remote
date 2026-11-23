#include "llremotebridge.h"
#include "../streaming/video/decoder.h"
#include <QCoreApplication>
#include <QDateTime>
#include <QEventLoop>
#include <QJsonDocument>
#include <QLocalServer>
#include <QLocalSocket>
#include <QThread>
#include <QTimer>
#include <atomic>
#include <deque>
#include <memory>
#include <mutex>
#ifdef Q_OS_WIN32
#define NOMINMAX
#include <windows.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>
#include <wrl/client.h>
#endif

namespace {
std::atomic<bool> connected{false};
std::atomic<bool> transportConnected{false};
std::atomic<bool> enabled{false};
std::atomic<bool> settingsVisible{false};
std::mutex metricsLock;
QJsonObject metrics;

enum class RequestState { Pending, Running, Succeeded, Failed };
struct Request {
    QJsonObject data;
    std::atomic<RequestState> state{RequestState::Pending};
};
std::mutex requestLock;
std::deque<std::shared_ptr<Request>> requests;

void respond(QLocalSocket* socket, bool ok) {
    QJsonObject response{{"version", 1}, {"ok", ok}};
    if (ok) {
        response["connected"] = connected.load();
        response["settingsVisible"] = settingsVisible.load();
        std::lock_guard<std::mutex> guard(metricsLock);
        response["metrics"] = metrics;
    }
    socket->write(QJsonDocument(response).toJson(QJsonDocument::Compact) + '\n');
    socket->disconnectFromServer();
}

bool tokenMatches(const QByteArray& expected, const QByteArray& actual) {
    if (expected.size() != actual.size()) return false;
    unsigned char diff = 0;
    for (int i = 0; i < expected.size(); ++i) diff |= expected.at(i) ^ actual.at(i);
    return diff == 0;
}
}

void LLRemoteBridge::start() {
    const QByteArray pipe = qgetenv("LLREMOTE_PIPE_NAME");
    const QByteArray token = qgetenv("LLREMOTE_SESSION_TOKEN");
    if (!pipe.startsWith("LLRemote-") || token.size() != 64) return;
    for (char c : pipe) if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-')) return;
    enabled.store(true);
    auto thread = QThread::create([pipe, token] {
        QLocalServer server;
        server.setSocketOptions(QLocalServer::UserAccessOption);
        QObject::connect(&server, &QLocalServer::newConnection, &server, [&server, token] {
            while (auto socket = server.nextPendingConnection()) {
                QObject::connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
                QObject::connect(socket, &QLocalSocket::readyRead, socket, [socket, token] {
                    if (socket->property("ll-request-received").toBool()) return;
                    auto buffer = socket->property("ll-buffer").toByteArray();
                    buffer += socket->read(65537 - buffer.size());
                    if (buffer.size() > 65536) { socket->disconnectFromServer(); return; }
                    const int newline = buffer.indexOf('\n');
                    if (newline < 0) { socket->setProperty("ll-buffer", buffer); return; }
                    socket->setProperty("ll-request-received", true);
                    const auto request = QJsonDocument::fromJson(buffer.left(newline)).object();
                    const auto action = request["action"].toString();
                    const bool authorized = request["version"].toInt() == 1 &&
                        tokenMatches(token, request["token"].toString().toUtf8());
                    if (!authorized || action == "status") {
                        respond(socket, authorized);
                        return;
                    }
                    if (action != "apply" && action != "disconnect" && action != "input") {
                        respond(socket, false);
                        return;
                    }
                    const auto command = std::make_shared<Request>();
                    command->data = request;
                    {
                        std::lock_guard<std::mutex> guard(requestLock);
                        if (!connected.load() || requests.size() >= 16) {
                            respond(socket, false);
                            return;
                        }
                        SDL_Event event{};
                        event.type = SDL_USEREVENT;
                        event.user.code = EventCode;
                        if (SDL_PushEvent(&event) != 1) {
                            respond(socket, false);
                            return;
                        }
                        requests.push_back(command);
                    }
                    QObject::connect(socket, &QLocalSocket::disconnected, socket, [command] {
                        auto pending = RequestState::Pending;
                        command->state.compare_exchange_strong(pending, RequestState::Failed);
                    });
                    auto timer = new QTimer(socket);
                    QObject::connect(timer, &QTimer::timeout, socket, [socket, timer, command] {
                        const auto state = command->state.load();
                        if (state == RequestState::Pending || state == RequestState::Running) return;
                        timer->stop();
                        respond(socket, state == RequestState::Succeeded);
                    });
                    timer->start(10);
                });
            }
        });
        if (!server.listen(QString::fromUtf8(pipe))) return;
        QEventLoop loop;
        loop.exec();
    });
    QObject::connect(QCoreApplication::instance(), &QCoreApplication::aboutToQuit, QCoreApplication::instance(), [thread] { thread->quit(); thread->wait(3000); });
    thread->start();
}

void LLRemoteBridge::setConnected(bool value) {
    transportConnected.store(value);
    if (!value) {
        std::lock_guard<std::mutex> guard(requestLock);
        connected.store(false);
        for (const auto& request : requests) request->state.store(RequestState::Failed);
        requests.clear();
    }
}

void LLRemoteBridge::processRequest(const std::function<bool(const QJsonObject&)>& apply) {
    std::shared_ptr<Request> request;
    {
        std::lock_guard<std::mutex> guard(requestLock);
        if (requests.empty()) return;
        request = std::move(requests.front());
        requests.pop_front();
    }
    auto pending = RequestState::Pending;
    if (request->state.compare_exchange_strong(pending, RequestState::Running)) {
        request->state.store(apply(request->data) ? RequestState::Succeeded : RequestState::Failed);
    }
}
void LLRemoteBridge::setSettingsVisible(bool visible) { settingsVisible.store(visible); }

void LLRemoteBridge::publishMetrics(const _VIDEO_STATS& stats) {
    if (!enabled.load() || !transportConnected.load()) return;
    if (!connected.load() && stats.renderedFrames > 0) {
        connected.store(true);
        fprintf(stdout, "LLREMOTE_STREAM_READY\n");
        fflush(stdout);
        bool ok = false;
        const int volume = qgetenv("LLREMOTE_AUDIO_VOLUME").toInt(&ok);
        applyAudio(ok ? qBound(0, volume, 100) : 75, qgetenv("LLREMOTE_AUDIO_MUTED") == "1");
    }
    QJsonObject data;
    data["fps"] = stats.renderedFps;
    data["bitrateMbps"] = stats.videoMegabitsPerSec;
    uint32_t rtt = 0, variance = 0;
    data["networkMs"] = LiGetEstimatedRttInfo(&rtt, &variance) ? QJsonValue(double(rtt)) : QJsonValue(QJsonValue::Null);
    data["decodeMs"] = stats.decodedFrames ? QJsonValue(double(stats.totalDecodeTimeUs) / stats.decodedFrames / 1000) : QJsonValue(QJsonValue::Null);
    data["renderMs"] = stats.renderedFrames ? QJsonValue(double(stats.totalRenderTimeUs) / stats.renderedFrames / 1000) : QJsonValue(QJsonValue::Null);
    data["droppedPercent"] = stats.totalFrames ? QJsonValue(double(stats.networkDroppedFrames + stats.pacerDroppedFrames) / stats.totalFrames * 100) : QJsonValue(QJsonValue::Null);
    data["updatedAt"] = QDateTime::currentDateTimeUtc().toString(Qt::ISODate);
    std::lock_guard<std::mutex> guard(metricsLock);
    metrics = data;
}

bool LLRemoteBridge::applyAudio(int volume, bool muted) {
#ifdef Q_OS_WIN32
    using Microsoft::WRL::ComPtr;
    const HRESULT init = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    ComPtr<IMMDeviceEnumerator> enumerator;
    bool applied = false;
    if (SUCCEEDED(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&enumerator)))) {
        ComPtr<IMMDeviceCollection> devices;
        if (SUCCEEDED(enumerator->EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE, &devices))) {
            UINT count = 0;
            devices->GetCount(&count);
            for (UINT i = 0; i < count; ++i) {
                ComPtr<IMMDevice> device;
                ComPtr<IAudioSessionManager2> manager;
                ComPtr<IAudioSessionEnumerator> sessions;
                if (FAILED(devices->Item(i, &device)) || FAILED(device->Activate(__uuidof(IAudioSessionManager2), CLSCTX_ALL, nullptr, &manager)) || FAILED(manager->GetSessionEnumerator(&sessions))) continue;
                int count = 0;
                sessions->GetCount(&count);
                for (int n = 0; n < count; ++n) {
                    ComPtr<IAudioSessionControl> control;
                    ComPtr<IAudioSessionControl2> control2;
                    ComPtr<ISimpleAudioVolume> audio;
                    DWORD pid = 0;
                    if (FAILED(sessions->GetSession(n, &control)) || FAILED(control.As(&control2)) || FAILED(control2->GetProcessId(&pid)) || pid != GetCurrentProcessId() || FAILED(control.As(&audio))) continue;
                    if (SUCCEEDED(audio->SetMasterVolume(float(qBound(0, volume, 100)) / 100, nullptr)) && SUCCEEDED(audio->SetMute(muted, nullptr))) applied = true;
                }
            }
        }
    }
    if (SUCCEEDED(init)) CoUninitialize();
    return applied;
#else
    Q_UNUSED(volume); Q_UNUSED(muted); return false;
#endif
}
