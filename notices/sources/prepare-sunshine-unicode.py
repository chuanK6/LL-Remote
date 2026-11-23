"""Make the bundled Sunshine use UTF-8 for argv and narrow filesystem paths.

The modified program is unsigned; the upstream signature cannot cover a changed
manifest. The original source archive and this reproducible manifest are shipped.
"""
from pathlib import Path
import ctypes
import hashlib
import json
import xml.etree.ElementTree as ET
import pefile

root = Path(__file__).resolve().parents[1]
executable = root / "engines/Sunshine/sunshine.exe"
manifest_path = root / "engines/manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
original = executable.read_bytes()
image = pefile.PE(data=original)
resource = None
for kind in image.DIRECTORY_ENTRY_RESOURCE.entries:
    if kind.id == 24:
        resource = kind.directory.entries[0].directory.entries[0]
        break
if resource is None:
    raise SystemExit("Sunshine has no Windows process manifest")
raw = image.get_data(resource.data.struct.OffsetToData, resource.data.struct.Size)
tree = ET.fromstring(raw)
ns = "urn:schemas-microsoft-com:asm.v3"
codepage_ns = "http://schemas.microsoft.com/SMI/2019/WindowsSettings"
if tree.find(f".//{{{codepage_ns}}}activeCodePage") is None:
    backup = root / ".work/sunshine-upstream-original.exe"
    backup.parent.mkdir(parents=True, exist_ok=True)
    backup.write_bytes(original)
    manifest["sunshine"]["upstreamSha256"] = hashlib.sha256(original).hexdigest()
    application = tree.find(f"{{{ns}}}application")
    if application is None: application = ET.SubElement(tree, f"{{{ns}}}application")
    settings = application.find(f"{{{ns}}}windowsSettings")
    if settings is None: settings = ET.SubElement(application, f"{{{ns}}}windowsSettings")
    ET.SubElement(settings, f"{{{codepage_ns}}}activeCodePage").text = "UTF-8"
    ET.register_namespace("", "urn:schemas-microsoft-com:asm.v1")
    ET.register_namespace("asmv3", ns)
    ET.register_namespace("ws2019", codepage_ns)
    updated = ET.tostring(tree, encoding="utf-8", xml_declaration=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.BeginUpdateResourceW.argtypes = [ctypes.c_wchar_p, ctypes.c_int]
    kernel.BeginUpdateResourceW.restype = ctypes.c_void_p
    kernel.UpdateResourceW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ushort, ctypes.c_void_p, ctypes.c_uint32]
    kernel.UpdateResourceW.restype = ctypes.c_int
    kernel.EndUpdateResourceW.argtypes = [ctypes.c_void_p, ctypes.c_int]
    kernel.EndUpdateResourceW.restype = ctypes.c_int
    handle = kernel.BeginUpdateResourceW(str(executable), 0)
    if not handle: raise ctypes.WinError(ctypes.get_last_error())
    buffer = ctypes.create_string_buffer(updated)
    if not kernel.UpdateResourceW(handle, ctypes.c_void_p(24), ctypes.c_void_p(1), resource.id, buffer, len(updated)):
        kernel.EndUpdateResourceW(handle, 1)
        raise ctypes.WinError(ctypes.get_last_error())
    if not kernel.EndUpdateResourceW(handle, 0): raise ctypes.WinError(ctypes.get_last_error())
    # The upstream Authenticode certificate is invalid after any resource change.
    modified = pefile.PE(str(executable))
    certificate = modified.OPTIONAL_HEADER.DATA_DIRECTORY[4]
    offset, length = certificate.VirtualAddress, certificate.Size
    certificate.VirtualAddress = certificate.Size = 0
    data = modified.write()
    if length and offset + length == len(data): data = data[:offset]
    modified = pefile.PE(data=data)
    modified.OPTIONAL_HEADER.CheckSum = modified.generate_checksum()
    executable.write_bytes(modified.write())
else:
    updated = raw
output = root / "notices/sources/Sunshine-LLRemote-UTF8.manifest"
output.write_bytes(updated)
(output.parent / "prepare-sunshine-unicode.py").write_bytes(Path(__file__).read_bytes())
manifest["sunshine"]["runtimeManifest"] = "utf8"
manifest["sunshine"]["sha256"] = hashlib.sha256(executable.read_bytes()).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"utf8Manifest": True, "modifiedBinaryUnsigned": True, "sourceManifestProvided": True}))
