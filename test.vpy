import vapoursynth as vs
from vapoursynth import core
from pathlib import Path

bl_path = Path("BL.mkv")
bl = core.ffms2.Source(bl_path)
bl = bl.resize.Spline36(format=vs.YUV420P16)

bl_w = bl.width
bl_h = bl.height

el_path = Path("EL.mkv")
el = core.ffms2.Source(el_path)
el = core.resize.Point(el, width=bl_w, height=bl_h, format=vs.YUV420P10)

hdr = core.dovi.DOVI(bl, el, "RPU.bin")
hdr = hdr.std.SetFrameProps(_Matrix=9, _Primaries=9, _Transfer=16)

hdr.set_output()
