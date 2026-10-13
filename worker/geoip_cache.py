from __future__ import annotations

import json
import sys
import threading
import time
from dataclasses import asdict

_MAX_AGE = 3 * 24 * 60 * 60
_LOCK = threading.Lock()


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
