import Darwin
import Foundation
import Network
import UIKit

private struct DiscoveryConfig: Decodable {
    let serviceType: String
    let port: UInt16
    let id: String
    let name: String
    let platform: String
    let version: String
    let protocolVersion: UInt16
    let minProtocolVersion: UInt16
    let capabilities: String
}

private struct ServiceIdentity: Hashable {
    let name: String
    let type: String
    let domain: String

    var fullname: String {
        "\(name).\(type.withTrailingDot)\(domain.withTrailingDot)"
    }
}

private extension String {
    var withTrailingDot: String {
        hasSuffix(".") ? self : self + "."
    }

    var withoutTrailingDot: String {
        hasSuffix(".") ? String(dropLast()) : self
    }
}

private final class NeloaBonjourDiscovery: NSObject, NetServiceDelegate {
    private let config: DiscoveryConfig
    // NetService delivers delegate callbacks through a run loop, so keep both
    // Foundation Bonjour and NWBrowser on the application's main run loop.
    private let queue = DispatchQueue.main
    private var browser: NWBrowser?
    private var publisher: NetService?
    private var visibleServices: [ServiceIdentity: NWBrowser.Result] = [:]
    private var resolvers: [ServiceIdentity: NetService] = [:]
    private var advertising = false
    private var browsing = false
    private var publisherError: String?
    private var browserError: String?
    private var lifecycleObservers: [NSObjectProtocol] = []
    private var browserRetry: DispatchWorkItem?

    init(config: DiscoveryConfig) {
        self.config = config
    }

    func start() {
        queue.async { [self] in
            observeApplicationLifecycle()
            if UIApplication.shared.applicationState == .active {
                resumeDiscovery()
            } else {
                reportStatus()
            }
        }
    }

    deinit {
        lifecycleObservers.forEach(NotificationCenter.default.removeObserver)
        browserRetry?.cancel()
    }

    private func observeApplicationLifecycle() {
        guard lifecycleObservers.isEmpty else { return }
        let center = NotificationCenter.default
        lifecycleObservers.append(
            center.addObserver(
                forName: UIApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.resumeDiscovery()
            }
        )
        lifecycleObservers.append(
            center.addObserver(
                forName: UIApplication.willResignActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.suspendDiscovery()
            }
        )
    }

    private func resumeDiscovery() {
        browserRetry?.cancel()
        browserRetry = nil
        publisherError = nil
        browserError = nil
        if publisher == nil {
            startPublishing()
        }
        if browser == nil {
            startBrowsing()
        }
        reportStatus()
    }

    private func suspendDiscovery() {
        browserRetry?.cancel()
        browserRetry = nil
        browser?.cancel()
        browser = nil
        publisher?.stop()
        publisher = nil
        resolvers.values.forEach { $0.stop() }
        resolvers.removeAll()
        visibleServices.keys.forEach { identity in
            identity.fullname.withCString { neloa_ios_discovery_peer_remove($0) }
        }
        visibleServices.removeAll()
        advertising = false
        browsing = false
        publisherError = nil
        browserError = nil
        reportStatus()
    }

    private func restartBrowserAfterDefunctConnection() {
        guard UIApplication.shared.applicationState == .active else { return }
        browserRetry?.cancel()
        let retry = DispatchWorkItem { [weak self] in
            guard let self, UIApplication.shared.applicationState == .active else { return }
            self.browserRetry = nil
            self.browserError = nil
            if self.browser == nil {
                self.startBrowsing()
            }
            self.reportStatus()
        }
        browserRetry = retry
        queue.asyncAfter(deadline: .now() + 1, execute: retry)
    }

    private func startPublishing() {
        let service = NetService(
            domain: "local.",
            type: config.serviceType.withTrailingDot,
            name: "Neloa-\(config.id.prefix(8))",
            port: Int32(config.port)
        )
        service.includesPeerToPeer = true
        service.delegate = self
        service.setTXTRecord(NetService.data(fromTXTRecord: txtRecord()))
        publisher = service
        service.publish(options: [.noAutoRename])
    }

    private func startBrowsing() {
        let parameters = NWParameters.udp
        parameters.includePeerToPeer = true
        let browser = NWBrowser(
            for: .bonjour(type: config.serviceType.withoutTrailingDot, domain: "local."),
            using: parameters
        )
        browser.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            self.queue.async {
                guard self.browser === browser else { return }
                switch state {
                case .ready:
                    self.browsing = true
                    self.browserError = nil
                case .waiting(let error):
                    self.browsing = false
                    self.browserError = self.browserMessage(for: error)
                case .failed(let error):
                    self.browsing = false
                    self.browserError = self.browserMessage(for: error)
                    browser.cancel()
                    self.browser = nil
                    if case .dns(let code) = error, code == -65569 {
                        self.restartBrowserAfterDefunctConnection()
                    }
                case .cancelled:
                    self.browsing = false
                default:
                    break
                }
                self.reportStatus()
            }
        }
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            self?.queue.async {
                self?.apply(results: results)
            }
        }
        self.browser = browser
        browser.start(queue: queue)
    }

    private func apply(results: Set<NWBrowser.Result>) {
        var next: [ServiceIdentity: NWBrowser.Result] = [:]
        for result in results {
            guard let identity = serviceIdentity(for: result.endpoint) else { continue }
            next[identity] = result
        }

        for identity in visibleServices.keys where next[identity] == nil {
            resolvers.removeValue(forKey: identity)?.stop()
            identity.fullname.withCString { neloa_ios_discovery_peer_remove($0) }
        }
        for (identity, result) in next where visibleServices[identity] != result {
            resolvers.removeValue(forKey: identity)?.stop()
            resolve(identity)
        }
        visibleServices = next
    }

    private func resolve(_ identity: ServiceIdentity) {
        let service = NetService(
            domain: identity.domain.withTrailingDot,
            type: identity.type.withTrailingDot,
            name: identity.name
        )
        service.includesPeerToPeer = true
        service.delegate = self
        resolvers[identity] = service
        service.resolve(withTimeout: 5)
    }

    private func serviceIdentity(for endpoint: NWEndpoint) -> ServiceIdentity? {
        guard case let .service(name: name, type: type, domain: domain, interface: _) = endpoint else {
            return nil
        }
        return ServiceIdentity(name: name, type: type, domain: domain)
    }

    private func txtRecord() -> [String: Data] {
        let values = [
            "id": config.id,
            "name": config.name,
            "platform": config.platform,
            "version": config.version,
            "protocolVersion": String(config.protocolVersion),
            "minProtocolVersion": String(config.minProtocolVersion),
            "capabilities": config.capabilities,
        ]
        return values.mapValues { Data($0.utf8) }
    }

    private func serviceIdentity(for service: NetService) -> ServiceIdentity {
        ServiceIdentity(name: service.name, type: service.type, domain: service.domain)
    }

    private func txtValue(_ key: String, from values: [String: Data]) -> String? {
        guard let data = values[key], !data.isEmpty else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func ipv4Addresses(from service: NetService) -> [String] {
        let addresses = (service.addresses ?? []).compactMap { data -> String? in
            data.withUnsafeBytes { bytes -> String? in
                guard let address = bytes.baseAddress?.assumingMemoryBound(to: sockaddr.self),
                      address.pointee.sa_family == sa_family_t(AF_INET) else {
                    return nil
                }
                var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                let length = socklen_t(address.pointee.sa_len)
                let result = host.withUnsafeMutableBufferPointer { buffer in
                    getnameinfo(
                        address,
                        length,
                        buffer.baseAddress,
                        socklen_t(buffer.count),
                        nil,
                        0,
                        NI_NUMERICHOST
                    )
                }
                return result == 0 ? String(cString: host) : nil
            }
        }
        return Array(Set(addresses)).sorted()
    }

    private func reportPeer(_ service: NetService) {
        guard let txtData = service.txtRecordData() else { return }
        let values = NetService.dictionary(fromTXTRecord: txtData)
        guard let id = txtValue("id", from: values), !id.isEmpty, id != config.id else {
            return
        }
        let addresses = ipv4Addresses(from: service)
        guard !addresses.isEmpty, service.port > 0 else { return }

        let payload: [String: Any] = [
            "id": id,
            "name": txtValue("name", from: values) ?? "Neloa Device",
            "platform": txtValue("platform", from: values) ?? "unknown",
            "version": txtValue("version", from: values) ?? "unknown",
            "protocolVersion": UInt16(txtValue("protocolVersion", from: values) ?? "") ?? 0,
            "minProtocolVersion": UInt16(txtValue("minProtocolVersion", from: values) ?? "") ?? 0,
            "capabilities": (txtValue("capabilities", from: values) ?? "")
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty },
            "addresses": addresses,
            "port": service.port,
            "serviceFullname": serviceIdentity(for: service).fullname,
        ]
        sendJSON(payload, to: neloa_ios_discovery_peer_upsert)
    }

    private func browserMessage(for error: NWError) -> String {
        if case .dns(let code) = error, code == -65570 {
            return "本地网络访问被拒绝，请前往系统设置允许 Neloa 访问本地网络"
        }
        return "无法浏览 Bonjour 服务：\(error.localizedDescription)"
    }

    private func reportStatus() {
        let error: Any
        if let message = browserError ?? publisherError {
            error = message
        } else {
            error = NSNull()
        }
        let payload: [String: Any] = [
            "advertising": advertising,
            "browsing": browsing,
            "error": error,
        ]
        sendJSON(payload, to: neloa_ios_discovery_status)
    }

    private func sendJSON(
        _ payload: [String: Any],
        to callback: (UnsafePointer<CChar>?) -> Void
    ) {
        guard JSONSerialization.isValidJSONObject(payload),
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else {
            return
        }
        json.withCString { callback($0) }
    }

    func netServiceDidPublish(_ sender: NetService) {
        queue.async { [weak self] in
            self?.advertising = true
            self?.publisherError = nil
            self?.reportStatus()
        }
    }

    func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
        queue.async { [weak self] in
            self?.advertising = false
            self?.publisherError = "无法发布 Bonjour 服务：\(errorDict)"
            self?.reportStatus()
        }
    }

    func netServiceDidResolveAddress(_ sender: NetService) {
        queue.async { [weak self] in
            guard let self else { return }
            self.reportPeer(sender)
            self.resolvers.removeValue(forKey: self.serviceIdentity(for: sender))
        }
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        queue.async { [weak self] in
            guard let self else { return }
            self.resolvers.removeValue(forKey: self.serviceIdentity(for: sender))
        }
    }
}

private enum NeloaBonjourRuntime {
    static var discovery: NeloaBonjourDiscovery?
}

@_cdecl("neloa_ios_discovery_start")
public func neloaIOSDiscoveryStart(_ configJSON: UnsafePointer<CChar>?) -> Int32 {
    guard let configJSON,
          let data = String(cString: configJSON).data(using: .utf8),
          let config = try? JSONDecoder().decode(DiscoveryConfig.self, from: data) else {
        return 1
    }
    guard NeloaBonjourRuntime.discovery == nil else { return 2 }
    let discovery = NeloaBonjourDiscovery(config: config)
    NeloaBonjourRuntime.discovery = discovery
    discovery.start()
    return 0
}
