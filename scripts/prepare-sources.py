from pathlib import Path
import urllib.request, zipfile, json, hashlib

root = Path(__file__).resolve().parent.parent
notices = root / 'notices'
sources = notices / 'sources'
sources.mkdir(parents=True, exist_ok=True)
sunshine_license = 'https://raw.githubusercontent.com/LizardByte/Sunshine/v2026.914.233613/LICENSE'
request = urllib.request.Request(sunshine_license, headers={'User-Agent': 'LLRemote-Development'})
if not (notices / 'Sunshine-GPLv3.txt').exists():
    (notices / 'Sunshine-GPLv3.txt').write_bytes(urllib.request.urlopen(request, timeout=30).read())
archive = sources / 'Sunshine-v2026.914.233613.zip'
if not archive.exists():
    url = 'https://codeload.github.com/LizardByte/Sunshine/zip/refs/tags/v2026.914.233613'
    request = urllib.request.Request(url, headers={'User-Agent': 'LLRemote-Development'})
    with urllib.request.urlopen(request, timeout=60) as response, archive.open('wb') as output:
        while chunk := response.read(1024 * 1024): output.write(chunk)

moonlight = root / 'third-party/sources/moonlight'
with zipfile.ZipFile(sources / 'Moonlight-v6.2.0-LLRemote.zip', 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    for path in moonlight.rglob('*'):
        relative = path.relative_to(moonlight)
        if path.is_file() and not any(part == '.git' for part in relative.parts) and not str(relative).startswith('build'):
            archive.write(path, str(Path('Moonlight-v6.2.0-LLRemote') / relative))
    for path in (root / 'third-party/moonlight-extension').glob('*'):
        if path.is_file(): archive.write(path, str(Path('LLRemote-extension') / path.name))
manifest = []
for path in sources.glob('*.zip'):
    manifest.append({'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes': path.stat().st_size})
(sources / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print('Prepared matching Moonlight and Sunshine source archives.')
