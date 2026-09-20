"""Create a complete bundle-only integrity manifest; reject links/reparse points."""
import argparse
import hashlib
import json
from pathlib import Path, PureWindowsPath
import stat
import sys


def without_system_ucrt(binaries):
    """Windows 10+ always loads its system UCRT; preserve every other TOC entry."""
    return [item for item in binaries
            if PureWindowsPath(item[0]).name.casefold() != 'ucrtbase.dll']


def validate_runtime_files(entries):
    names = {PureWindowsPath(item['path']).name.casefold() for item in entries}
    if 'ucrtbase.dll' in names:
        raise ValueError('system_ucrt_must_not_be_bundled')
    required = {'vcruntime140.dll', '_frida.pyd',
                f'python{sys.version_info.major}{sys.version_info.minor}.dll'}
    if not required.issubset(names):
        raise ValueError('bundle_runtime_missing')


def bundle_files(bundle):
    bundle = Path(bundle).absolute()
    entries = []
    paths = [bundle]
    seen = set()
    while paths:
        path = paths.pop()
        metadata = path.lstat()
        if path.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400:
            raise ValueError('bundle_reparse_point')
        if stat.S_ISREG(metadata.st_mode):
            relative = path.relative_to(bundle).as_posix()
            if relative.casefold() in seen:
                raise ValueError('bundle_duplicate_path')
            seen.add(relative.casefold())
            entries.append({'path': relative, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
        elif stat.S_ISDIR(metadata.st_mode):
            paths.extend(path.iterdir())
        else:
            raise ValueError('bundle_non_regular_entry')
    if not any(item['path'] == 'SayAllKeyHelper.exe' for item in entries):
        raise ValueError('helper_executable_missing')
    return sorted(entries, key=lambda item: item['path'])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('bundle', type=Path)
    parser.add_argument('manifest', type=Path)
    args = parser.parse_args()
    if args.manifest.absolute().is_relative_to(args.bundle.absolute()):
        raise ValueError('manifest_must_be_outside_bundle')
    manifest = {'schema': 1, 'name': 'SayAllKeyHelper', 'frida_version': '17.18.0',
                'python_version': '.'.join(map(str, sys.version_info[:3])), 'files': bundle_files(args.bundle)}
    validate_runtime_files(manifest['files'])
    args.manifest.write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'bundle_file_count': len(manifest['files']), 'manifest': 'passed'}))


if __name__ == '__main__':
    main()
