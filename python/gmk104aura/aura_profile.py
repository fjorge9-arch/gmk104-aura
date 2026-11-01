"""Leitura do estado atual do ASUS Aura Sync a partir dos arquivos do LightingService.

- LastProfile.xml: perfil atual. mode key="100" = Smart (térmico); nos demais modos usamos a cor
  base do LED 0 da placa-mãe (Mainboard_Master).
- script/LastScript.xml: no modo Smart, limiares efetivos (Threshold_low/high) e cores
  (hue/saturation/lightness) de cada faixa (Thermal_Low / Thermal_Between / Thermal_High).

Os dois arquivos são reescritos pelo LightingService a cada mudança no Armoury Crate; só relemos
quando o mtime muda.
"""
from __future__ import annotations

import colorsys
import logging
import os
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field

log = logging.getLogger("gmk104.aura")

LS_DIR = os.environ.get("GMK104_LS_DIR", r"C:\Program Files (x86)\LightingService")
PROFILE = os.path.join(LS_DIR, "LastProfile.xml")
SCRIPT = os.path.join(LS_DIR, "script", "LastScript.xml")
MODE_SMART = "100"


@dataclass
class HSL:
    h: float  # 0..1
    s: float  # 0..1
    l: float  # 0..1

    def to_hs255(self) -> tuple[int, int]:
        """HSL -> (hue, sat) em 0..255 para o teclado. O brilho (V) fica a cargo do ajuste do teclado."""
        v = self.l + self.s * min(self.l, 1 - self.l)
        sv = 0.0 if v == 0 else 2 * (1 - self.l / v)
        return int(round(self.h * 255)) % 256, max(0, min(255, int(round(sv * 255))))


@dataclass
class AuraState:
    mode: str = ""
    smart: bool = False
    thr_low: float = 65
    thr_high: float = 85
    smart_colors: dict[str, HSL] = field(default_factory=dict)
    static: HSL = field(default_factory=lambda: HSL(0, 1, 0.5))

    def band(self, temp_c: float) -> str:
        if temp_c < self.thr_low:
            return "Thermal_Low"
        if temp_c > self.thr_high:
            return "Thermal_High"
        return "Thermal_Between"

    def color_for_temp(self, temp_c: float | None) -> HSL:
        if not self.smart or temp_c is None:
            return self.static
        return self.smart_colors.get(self.band(temp_c), self.static)


def _color_int_to_hsl(v: int) -> HSL:
    r, g, b = v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF  # formato 0x00BBGGRR
    h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
    return HSL(h, s, l)


def parse_profile(path: str = PROFILE) -> AuraState:
    st = AuraState()
    root = ET.parse(path).getroot()
    scene = next((s for s in root.iter("scene") if s.get("key") == "S0"), None) or next(root.iter("scene"))
    mode = next(scene.iter("mode"))
    st.mode = mode.get("key", "")
    st.smart = st.mode == MODE_SMART
    for led in mode.iter("led"):
        if led.findtext("DeviceName") != "Mainboard_Master":
            continue
        try:
            h, s, l = (float(led.findtext(k)) for k in ("hue", "saturation", "lightness"))
            if s > 0 or l > 0:
                st.static = HSL(h, s, l)
            else:
                st.static = _color_int_to_hsl(int(led.findtext("color") or 0))
        except (TypeError, ValueError):
            st.static = _color_int_to_hsl(int(led.findtext("color") or 0))
        break
    t1, t2 = root.findtext(".//thermal_threshold_one"), root.findtext(".//thermal_threshold_two")
    if t1 and t2:
        st.thr_low, st.thr_high = float(t1), float(t2)
    return st


def parse_script(st: AuraState, path: str = SCRIPT) -> AuraState:
    """Complementa com o script: limiares efetivos e cores por faixa (modo Smart)."""
    if not os.path.exists(path):
        return st
    root = ET.parse(path).getroot()
    effects = {e.get("key"): e for e in root.iter("effect")
               if (e.get("key") or "").startswith("SmartSingleEff")}
    for eff in root.iter("effect"):
        if eff.get("key") != "Smartmode":
            continue
        lo, hi = eff.findtext("Threshold_low"), eff.findtext("Threshold_high")
        if lo and hi:
            st.thr_low, st.thr_high = float(lo), float(hi)
        trig = eff.findtext("trigger")
        sub = eff.find("smartmode_effect")
        if trig and sub is not None and sub.get("key") in effects and trig not in st.smart_colors:
            e = effects[sub.get("key")]
            h = e.findtext(".//hue")
            if h is not None:
                st.smart_colors[trig] = HSL(float(h), float(e.findtext(".//saturation") or 1),
                                            float(e.findtext(".//lightness") or 0.5))
    return st


class AuraWatcher:
    """Relê os arquivos quando mudam; mantém o último estado válido."""

    def __init__(self) -> None:
        self._mtimes: tuple[float, float] = (-1.0, -1.0)
        self.state = AuraState()

    @staticmethod
    def _mtime(p: str) -> float:
        try:
            return os.path.getmtime(p)
        except OSError:
            return -1.0

    def poll(self) -> bool:
        m = (self._mtime(PROFILE), self._mtime(SCRIPT))
        if m == self._mtimes:
            return False
        try:
            st = parse_script(parse_profile())
        except Exception as e:  # arquivo sendo reescrito pelo LightingService, etc.
            log.warning("não consegui ler o perfil do Aura: %s", e)
            return False
        self.state, self._mtimes = st, m
        log.info("perfil Aura: mode=%s smart=%s limiares=%s/%s cores=%s estatica=%s",
                 st.mode, st.smart, st.thr_low, st.thr_high,
                 {k: v.to_hs255() for k, v in st.smart_colors.items()}, st.static.to_hs255())
        return True
