"""Sonda elevada: GITM / ItemsOfGroup no COM atkexCom.axdata."""
import os, time, ctypes, comtypes, comtypes.client, comtypes.automation
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "probe_temp_result.txt")
L = []
k32 = ctypes.windll.kernel32
ev = k32.OpenEventW(0x00100000 | 0x0002, False, "Global\ASUS_SIO_MUTEX")
def locked(fn):
    k32.WaitForSingleObject(ev, 10000)
    try: return fn()
    finally: k32.SetEvent(ev)
try:
    o = comtypes.client.CreateObject("atkexCom.axdata")
    L.append("init=%r" % o.iAcpiInit())
    # typelib: assinatura real
    try:
        import comtypes.typeinfo
        ti = o.GetTypeInfo(0)
        ta = ti.GetTypeAttr()
        for i in range(ta.cFuncs):
            fd = ti.GetFuncDesc(i)
            name = ti.GetDocumentation(fd.memid)[0]
            params = [(p.tdesc.vt, p.paramdesc.wParamFlags) for p in [fd.lprgelemdescParam[j] for j in range(fd.cParams)]]
            if name in ("iAcpiGetItem","GITM","iAcpiGetItemBuffer","ItemsOfGroup","GGRP","iAcpiGetItemEx"):
                L.append(f"sig {name}: ret vt={fd.elemdescFunc.tdesc.vt} params(vt,flags)={params}")
    except Exception as e: L.append("typeinfo ERR %r" % e)
    for id_ in (0x06030012, 0x06030000, 0x06000000):
        for name in ("GITM", "iAcpiGetItemBuffer"):
            try:
                if name == "GITM":
                    r = locked(lambda: o.GITM(id_, comtypes.automation.VARIANT()))
                else:
                    r = locked(lambda: o.iAcpiGetItemBuffer(id_, 0, comtypes.automation.VARIANT(), 0))
                L.append(f"{name}(0x{id_:08X}) -> {r!r}")
            except Exception as e: L.append(f"{name}(0x{id_:08X}) ERR {e!r}")
    for gid in (0x06030000, 0x00030000, 0x06000000, 0x00010000, 0x00060000, 0x06030012):
        try:
            col = locked(lambda: o.ItemsOfGroup(gid))
            n = col.Count
            L.append(f"ItemsOfGroup(0x{gid:08X}) count={n}")
            for i in range(min(n, 40)):
                it = col.Item(i)
                props = {}
                for p in ("Name","ID","Id","Value","Flags","Type","Unit","Min","Max","Step","Default","Desc"):
                    try: props[p] = getattr(it, p)
                    except Exception: pass
                L.append("   " + repr(props))
        except Exception as e: L.append(f"ItemsOfGroup(0x{gid:08X}) ERR {e!r}")
except Exception as e:
    L.append("ERR %r" % e)
open(out, "w", encoding="utf-8").write("\n".join(L) + "\n")
