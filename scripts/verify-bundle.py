from pathlib import Path
import pefile, json, os, sys, hashlib, argparse
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser(description='Verify native imports, pinned engines, and optional complete package resource hashes.')
parser.add_argument('root', nargs='?', default='.')
parser.add_argument('--reference', help='Compare all packaged engine and runtime files with this build workspace.')
parser.add_argument('--require-clean-data', action='store_true', help='Require a distribution with an empty Data directory and no native user configuration.')
args = parser.parse_args()
root = Path(args.root).resolve()
engines = root / 'engines'
windows = Path(os.environ.get('WINDIR', 'C:/Windows'))
directories = [root, engines / 'Moonlight', engines / 'Sunshine', engines / 'Jiaoyuelian']
runtime = root / 'runtimes' / 'WebView2'
missing = []
private_extensions = {'.pem','.key','.p12','.pfx','.log','.ini'}
private_names = {'config','sunshine_state.json','preferences.json','devices.json'}
def private_engine_file(path, base):
    relative = path.relative_to(base)
    return path.suffix.lower() in private_extensions or path.name.lower() in private_names or any(part.lower() in {'config','data','logs','cache'} for part in relative.parts[:-1])
if args.require_clean_data:
    if any(path.is_file() for path in (root / 'Data').rglob('*')):
        missing.append({'resource':'Data', 'error':'The distribution contains local application data'})
    for path in engines.rglob('*'):
        if path.is_file() and private_engine_file(path, engines):
            missing.append({'resource':str(path.relative_to(root)), 'error':'Native user configuration must not be distributed'})
if runtime.exists() or (root / 'WebView2').exists():
    missing.append({'resource': 'runtimes/WebView2', 'error': 'The package must use system Evergreen WebView2 and must not ship a browser runtime'})
checked = 0
runtime_checked = 0
directory_cache = {}
application = root / 'LLRemote.exe'
if application.is_file():
    executable = pefile.PE(str(application))
    manifests = []
    resources = getattr(executable, 'DIRECTORY_ENTRY_RESOURCE', None)
    for resource in (resources.entries if resources else []):
        if resource.id == 24:
            for item in resource.directory.entries:
                for language in item.directory.entries:
                    data = language.data.struct
                    manifests.append(executable.get_data(data.OffsetToData, data.Size))
    try:
        if len(manifests) != 1:
            raise ValueError('The application must have one Windows manifest')
        application_manifest = ET.fromstring(manifests[0])
        package_manifest = root / 'component-manifest.json'
        if package_manifest.is_file():
            expected = json.loads(package_manifest.read_text(encoding='utf-8-sig')).get('windows', {}).get('requestedExecutionLevel')
            level = application_manifest.find('.//{urn:schemas-microsoft-com:asm.v3}requestedExecutionLevel')
            if expected and (level is None or level.get('level') != expected):
                raise ValueError('Windows execution level does not match the package manifest')
    except (ET.ParseError, ValueError) as error:
        missing.append({'binary': 'LLRemote.exe', 'error': str(error)})
    executable.close()
def names_in(directory):
    if directory not in directory_cache:
        directory_cache[directory] = {p.name.lower() for p in directory.iterdir() if p.is_file()} if directory.is_dir() else set()
    return directory_cache[directory]

for directory in directories:
    for path in directory.iterdir() if directory == root else directory.rglob('*'):
        if not path.is_file() or path.suffix.lower() not in ['.exe', '.dll']:
            continue
        checked += 1
        runtime_checked += int(directory == runtime)
        try:
            binary = pefile.PE(str(path), fast_load=True)
            binary.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_IMPORT']])
            system = windows / ('SysWOW64' if binary.FILE_HEADER.Machine == 0x14c else 'System32')
            search = [path.parent, directory, root, system]
            for entry in getattr(binary, 'DIRECTORY_ENTRY_IMPORT', []):
                name = entry.dll.decode(errors='replace').lower()
                if name.startswith(('api-ms-', 'ext-ms-')) or any(name in names_in(candidate) for candidate in search):
                    continue
                missing.append({'binary': str(path.relative_to(root)), 'dependency': name})
            binary.close()
        except pefile.PEFormatError:
            pass
manifest = json.loads((engines / 'manifest.json').read_text(encoding='utf-8-sig'))
for component in ['moonlight', 'sunshine', 'jiaoyuelian']:
    entry = manifest[component]
    executable = engines / entry['executable']
    if not executable.is_file() or hashlib.sha256(executable.read_bytes()).hexdigest().lower() != entry['sha256'].lower():
        missing.append({'component': component, 'error': 'Pinned executable hash mismatch'})
compared = 0
nsis_marker_accepted = False
if args.reference:
    reference = Path(args.reference).resolve()
    pairs = [(reference / 'src-tauri/target/release/LLRemote.exe', root / 'LLRemote.exe'),
             (reference / 'LLRemote.EngineHost.exe', root / 'LLRemote.EngineHost.exe')]
    for source, target in [(reference / 'engines', engines),
                           (reference / 'runtimes/VC/x64', root)]:
        pairs.extend((path, target / path.relative_to(source)) for path in source.rglob('*') if path.is_file() and not (source == reference / 'engines' and private_engine_file(path, source)))
    def digest(path):
        value = hashlib.sha256()
        with path.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                value.update(chunk)
        return value.digest()
    for source, target in pairs:
        compared += 1
        matches = target.is_file() and digest(source) == digest(target)
        if not matches and target == root / 'LLRemote.exe' and target.is_file():
            source_bytes, target_bytes = source.read_bytes(), target.read_bytes()
            marker = b'__TAURI_BUNDLE_TYPE_VAR_UNK'
            matches = source_bytes.count(marker) == 1 and source_bytes.replace(marker, b'__TAURI_BUNDLE_TYPE_VAR_NSS', 1) == target_bytes
            nsis_marker_accepted = matches
        if not matches:
            missing.append({'resource': str(target.relative_to(root)), 'error': 'Build resource missing or hash mismatch'})
if missing:
    print(json.dumps({'checked': checked, 'missing': missing}, ensure_ascii=False, indent=2))
    sys.exit(1)
print(json.dumps({'checkedNativeBinaries': checked, 'bundledWebView2Binaries': runtime_checked, 'webview2Delivery': 'system-evergreen', 'nestedQtPluginsChecked': True, 'pinnedComponentHashesMatch': True, 'resourceFilesCompared': compared, 'nsisBundleMarkerAccepted': nsis_marker_accepted, 'missingDependencies': 0, 'systemBaseline': 'Current local Windows; clean Windows acceptance deferred'}, ensure_ascii=False))
