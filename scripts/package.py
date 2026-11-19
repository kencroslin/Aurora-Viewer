"""Package a verified native executable from the custom builds workflow."""
from pathlib import Path
import hashlib
import json
import os
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile


def run(*args):
    subprocess.run(args, check=True)


def capture(*args):
    return subprocess.check_output(args, text=True).strip()


def verify_windows_executable(path):
    with path.open('rb') as binary:
        if binary.read(2) != b'MZ':
            raise RuntimeError('Expected a Windows executable')
        binary.seek(0x3C)
        header_offset, = struct.unpack('<I', binary.read(4))
        binary.seek(header_offset)
        if binary.read(6) != b'PE\x00\x00\x64\x86':
            raise RuntimeError('Expected a native x64 Windows executable')


def resources(source, folder, metadata):
    for name in ('LICENSE', 'NOTICE.md', 'README.md', 'TRADEMARKS.md', 'THIRD-PARTY-LICENSES.md'):
        shutil.copy(source / name, folder / name)
    emoji = folder / 'assets/emoji'
    emoji.mkdir(parents=True)
    shutil.copy(source / 'assets/emoji/LICENSE-OFL.txt', emoji)
    dictionaries = source / 'crates/aurora-viewer/assets/dictionaries'
    dictionary_output = folder / 'assets/dictionaries'
    dictionary_output.mkdir()
    shutil.copy(dictionaries / 'README.md', dictionary_output)
    for license_file in dictionaries.glob('*/LICENSE.txt'):
        shutil.copy(license_file, dictionary_output / f'{license_file.parent.name}-LICENSE.txt')
    for name, data in (
        ('aurora-install.json', {'schema': 1, 'version': metadata['version']}),
        ('custom-build.json', metadata),
    ):
        (folder / name).write_text(json.dumps(data, indent=2) + '\n', encoding='utf-8')


def package_mac(source, exe, metadata, staging, archive):
    if 'arm64' not in capture('/usr/bin/file', '-b', exe):
        raise RuntimeError('Expected a native Apple Silicon executable')
    app = staging / 'Aurora Custom.app'
    contents = app / 'Contents'
    macos = contents / 'MacOS'
    macos.mkdir(parents=True)
    binary = macos / 'aurora-viewer'
    shutil.copy(exe, binary)
    binary.chmod(0o755)
    res = contents / 'Resources'
    res.mkdir()
    resources(source, res, metadata)
    packaging = source / 'packaging/macos'
    plist = plistlib.loads((packaging / 'Info.plist').read_bytes())
    number = metadata['version'].split('-')[0]
    plist.update(CFBundleDisplayName='Aurora Custom', CFBundleName='Aurora Custom',
                 CFBundleIdentifier='io.github.kencroslin.AuroraCustom',
                 CFBundleShortVersionString=number, CFBundleVersion=number)
    (contents / 'Info.plist').write_bytes(plistlib.dumps(plist))
    (contents / 'PkgInfo').write_bytes(b'APPL????')
    shutil.copy(source / 'assets/branding/aurora.icns', res / 'Aurora.icns')
    for localization in packaging.glob('*.lproj'):
        shutil.copytree(localization, res / localization.name, copy_function=shutil.copy)
    run('/usr/bin/plutil', '-lint', contents / 'Info.plist')
    run('/usr/bin/codesign', '--force', '--sign', '-', app)
    run('/usr/bin/codesign', '--verify', '--strict', app)
    run('/usr/bin/ditto', '-c', '-k', '--keepParent', app, archive)


def package_windows(source, exe, metadata, staging, archive):
    verify_windows_executable(exe)
    folder = staging / 'Aurora Custom'
    folder.mkdir()
    shutil.copy(exe, folder / 'aurora-viewer.exe')
    resources(source, folder, metadata)
    shutil.make_archive(str(archive.with_suffix('')), 'zip', staging, folder.name)


def main():
    source, dist = [Path(arg).resolve() for arg in sys.argv[1:]]
    version = os.environ['VERSION']
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+-custom\.[0-9]+(?:\.[0-9]+)?', version):
        raise ValueError('Unexpected build version')
    platform = os.environ['PACKAGE_PLATFORM']
    if platform not in ('macos-arm64', 'windows-x64'):
        raise ValueError('Unsupported package platform')
    exe_name = 'aurora-viewer.exe' if platform == 'windows-x64' else 'aurora-viewer'
    exe = source / 'target/debug' / exe_name
    if capture(exe, '--version') != f'Aurora Viewer {version}':
        raise RuntimeError('Unexpected compiled version')
    metadata = dict(version=version, platform=platform, source_commit=os.environ['SOURCE_COMMIT'],
                    upstream_commit=os.environ['UPSTREAM_COMMIT'])
    dist.mkdir(parents=True, exist_ok=True)
    archive = dist / f'Aurora-Custom-{version}-{platform}.zip'
    with tempfile.TemporaryDirectory(prefix='aurora-custom-package-') as directory:
        package = package_windows if platform == 'windows-x64' else package_mac
        package(source, exe, metadata, Path(directory), archive)
    with archive.open('rb') as file:
        digest = hashlib.file_digest(file, 'sha256').hexdigest()
    archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n', encoding='ascii')


if __name__ == '__main__':
    main()
