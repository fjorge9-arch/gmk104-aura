"""Sonda elevada: dump dos sensores do COM aaHM.acpiHmData2 (via dir())."""
import os, time, comtypes, comtypes.client
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "probe_hm_result.txt")
L = []
SKIP = {"AddRef","Release","QueryInterface","GetTypeInfo","GetTypeInfoCount","GetIDsOfNames","Invoke","from_param","value"}
def dump(it):
    props = {}
    for n in dir(it):
        if n.startswith("_") or n in SKIP: continue
        try:
            v = getattr(it, n)
            if callable(v) and not isinstance(v, comtypes.client.lazybind.NamedProperty if hasattr(comtypes.client,'lazybind') else ()): 
                props[n] = "<method>"
            else:
                props[n] = v
        except Exception as e: props[n] = "ERR " + repr(e)[:50]
    return props
try:
    o = comtypes.client.CreateObject("aaHM.acpiHmData2")
    o.SetIsMonitoring(1); o.Refresh()
    s = o.Sensors
    L.append("Sensors type=%s dir=%r" % (type(s).__name__, [n for n in dir(s) if not n.startswith("_") and n not in SKIP]))
    n = s.Count; L.append(f"Count={n}")
    for i in range(n):
        try: it = s.Item(i)
        except Exception:
            try: it = s.Item(i+1)
            except Exception as e: L.append(f"[{i}] ERR {e!r}"); continue
        L.append(f"[{i}] {dump(it)!r}")
except Exception as e:
    import traceback; L.append("ERR " + traceback.format_exc())
open(out, "w", encoding="utf-8").write("\n".join(L) + "\n")
