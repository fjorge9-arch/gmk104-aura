"""Cor atual do Aura a partir do log do LightingService (não exige administrador).

No modo Smart o LightingService registra, a cada troca de faixa térmica (e a cada reaplicação do
perfil), a cor que passa a aplicar na placa-mãe:

    ... [ScriptEvent][ParseInitColor] parsed value => hue=0.333333, saturation=1.000000, lightness=0.500000, alpha=1.000000

O arquivo é público (%ProgramData%\\ASUS\\ARMOURY CRATE Diagnosis\\LightingService\\LightingService.log),
gravado com flush imediato e rotacionado a 10 MB (log4cxx: .log.1 … .log.10). Acompanhamos só o que
foi acrescentado desde a última leitura; se o arquivo encolher (rotação), recomeçamos do início.
Como a cor só é registrada quando muda, logo após uma rotação o arquivo atual pode não ter nenhuma
linha de cor: nesse caso procuramos a última nos backups (.1, .2, …).
"""
from __future__ import annotations

import logging
import os
import re

from .aura_profile import HSL

log = logging.getLogger("gmk104.auralog")

LOG_PATH = os.environ.get(
    "GMK104_LS_LOG",
    os.path.join(os.environ.get("PROGRAMDATA", r"C:\ProgramData"),
                 "ASUS", "ARMOURY CRATE Diagnosis", "LightingService", "LightingService.log"))

_RE = re.compile(rb"\[ParseInitColor\] parsed value => hue=([0-9.]+), saturation=([0-9.]+), lightness=([0-9.]+)")
TAIL_BYTES = 512 * 1024  # quanto ler do fim na primeira passada para achar a última cor
BACKUPS = 3              # quantos backups (.1, .2, .3) consultar quando o arquivo atual não tem cor


def _last_color_in(path: str, max_bytes: int | None = None) -> HSL | None:
    """Última cor registrada em `path` (lê só o fim, se max_bytes for dado)."""
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as f:
            if max_bytes is not None and size > max_bytes:
                f.seek(size - max_bytes)
            data = f.read()
    except OSError:
        return None
    last = None
    for m in _RE.finditer(data):
        last = m
    if last is None:
        return None
    try:
        return HSL(float(last.group(1)), float(last.group(2)), float(last.group(3)))
    except ValueError:
        return None


class AuraLogWatcher:
    """Mantém `color` = última cor registrada no log (None até achar uma)."""

    def __init__(self, path: str = LOG_PATH) -> None:
        self.path = path
        self.color: HSL | None = None
        self.available = False  # True quando o arquivo existe e já foi lido ao menos uma vez
        self._pos = -1
        self._buf = b""

    def _set(self, color: HSL, origem: str) -> bool:
        if self.color is not None and (color.h, color.s, color.l) == (self.color.h, self.color.s, self.color.l):
            return False
        self.color = color
        log.info("log do LightingService (%s): cor da placa hue=%.4f sat=%.2f light=%.2f -> teclado %s",
                 origem, color.h, color.s, color.l, color.to_hs255())
        return True

    def _from_backups(self) -> bool:
        for i in range(1, BACKUPS + 1):
            c = _last_color_in(f"{self.path}.{i}")  # inteiro: a cor pode estar bem antes do fim
            if c is not None:
                return self._set(c, f"backup .{i}")
        return False

    def poll(self) -> bool:
        """Lê o que há de novo; devolve True se a cor mudou."""
        try:
            size = os.path.getsize(self.path)
        except OSError:
            self.available = False
            return False
        first = self._pos < 0
        if first or size < self._pos:  # primeira vez ou rotação
            self._pos = max(0, size - TAIL_BYTES) if first else 0
            self._buf = b""
            if not first:
                log.info("log do LightingService rotacionou; relendo do início")
        if size == self._pos:
            if not self.available:
                self.available = True
                if self.color is None:
                    self._from_backups()
            return False
        try:
            with open(self.path, "rb") as f:
                f.seek(self._pos)
                data = f.read(size - self._pos)
        except OSError as e:
            log.debug("falha lendo log do LightingService: %s", e)
            return False
        self._pos += len(data)
        self.available = True
        data = self._buf + data
        last = None
        for m in _RE.finditer(data):
            last = m
        if last is None:
            self._buf = data[-256:]  # possível linha cortada no meio
            # arquivo atual (ou o que já lemos dele) sem cor: a última está num backup
            return self._from_backups() if self.color is None else False
        self._buf = data[last.end():][-256:]
        try:
            color = HSL(float(last.group(1)), float(last.group(2)), float(last.group(3)))
        except ValueError:
            return False
        return self._set(color, "atual")
