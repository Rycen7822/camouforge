from __future__ import annotations

import json
import queue
import sys
import threading
import time
from dataclasses import asdict

_MAX_AGE = 3 * 24 * 60 * 60
_LOCK = threading.Lock()


def get_public_ip(proxy: str | None = None) -> str:
    from ipaddress import ip_address

    import requests
    from camoufox.exceptions import InvalidIP
    from camoufox.ip import Proxy, _suppress_insecure_warning

    urls = (
        "https://api.ipify.org",
        "https://checkip.amazonaws.com",
        "https://ipinfo.io/ip",
        "https://icanhazip.com",
        "https://ifconfig.co/ip",
        "https://ipecho.net/plain",
    )
    proxies = Proxy.as_requests_proxy(proxy) if proxy else None
    results = queue.SimpleQueue()

    def fetch(url):
        try:
            with _suppress_insecure_warning():
                response = requests.get(url, proxies=proxies, timeout=5, verify=False)
            response.raise_for_status()
            ip = response.text.strip()
            ip_address(ip)
            results.put(ip)
        except Exception:
            results.put(None)

    # 慢服务不挡住备用结果；每次启动重新查询，最多并发两个请求。
    for offset in range(0, len(urls), 2):
        for url in urls[offset:offset + 2]:
            threading.Thread(target=fetch, args=(url,), daemon=True).start()
        for _ in range(2):
            result = results.get()
            if result is not None:
                return result
    raise InvalidIP("无法识别当前代理出口 IP，请检查网络或代理后重试")


def get_cached_geolocation(ip: str, geoip_db: str | None = None):
    from camoufox import geolocation
    from camoufox.locales import Geolocation, Locale

    source = geoip_db or geolocation.load_geoip_config()["name"]
    cache = geolocation.GEOIP_DIR / "lookup.json"
    with _LOCK:
        try:
            entry = json.loads(cache.read_text(encoding="utf-8"))
            if (
                entry["ip"] == ip
                and entry["source"] == source
                and 0 <= time.time() - entry["cached_at"] < _MAX_AGE
            ):
                cached = entry["result"]
                return Geolocation(locale=Locale(**cached.pop("locale")), **cached)
        except (OSError, ValueError, TypeError, KeyError):
            pass

        result = geolocation.get_geolocation(ip, geoip_db=geoip_db)
        entry = {
            "ip": ip,
            "source": source,
            "cached_at": time.time(),
            "result": asdict(result),
        }
        try:
            cache.parent.mkdir(parents=True, exist_ok=True)
            pending = cache.with_suffix(".tmp")
            pending.write_text(json.dumps(entry, ensure_ascii=False), encoding="utf-8")
            pending.replace(cache)
        except OSError:
            print("[camoforge] GeoIP 缓存未保存，本次仍使用新查询结果", file=sys.stderr)
        return result
