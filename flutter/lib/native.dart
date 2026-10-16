/// FFI binding to liblatex_ffi.
///
/// This is the only file in the Flutter app that talks to the native library.
/// Every function returns a JSON-decoded `Map<String, dynamic>`; the native
/// side never panics across the boundary and reports failures as
/// `{"error": "..."}`.
library;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

// ---------- signatures -------------------------------------------------------

typedef _CompileC = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _CompileDart = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);

typedef _IndexC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _IndexDart = Pointer<Utf8> Function(Pointer<Utf8>);

typedef _SaveC = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _SaveDart = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);

typedef _ListC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _ListDart = Pointer<Utf8> Function(Pointer<Utf8>);

typedef _ReadC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _ReadDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);

typedef _EnginesListC = Pointer<Utf8> Function();
typedef _EnginesListDart = Pointer<Utf8> Function();

typedef _TplListC = Pointer<Utf8> Function();
typedef _TplListDart = Pointer<Utf8> Function();

typedef _TplCreateC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _TplCreateDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);

typedef _PublishC = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _PublishDart = Pointer<Utf8> Function(
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);

typedef _VerifyC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _VerifyDart = Pointer<Utf8> Function(Pointer<Utf8>);

typedef _CorrectProposeC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _CorrectProposeDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);

typedef _CorrectApplyC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _CorrectApplyDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>);

typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

// ---------- binding ----------------------------------------------------------

class LatexNative {
  final DynamicLibrary _lib;

  late final _CompileDart _compile;
  late final _IndexDart _index;
  late final _SaveDart _saveAtomic;
  late final _SaveDart _autosaveWrite;
  late final _ListDart _autosaveList;
  late final _ReadDart _autosaveRead;
  late final _ReadDart _autosaveClear;
  late final _EnginesListDart _enginesList;
  late final _TplListDart _tplList;
  late final _TplCreateDart _tplCreate;
  late final _PublishDart _publishArchive;
  late final _VerifyDart _verifyProject;
  late final _CorrectProposeDart _correctPropose;
  late final _CorrectApplyDart _correctApply;
  late final _FreeDart _free;

  LatexNative._(this._lib) {
    _compile = _lib.lookupFunction<_CompileC, _CompileDart>('latex_compile');
    _index = _lib.lookupFunction<_IndexC, _IndexDart>('latex_project_index');
    _saveAtomic = _lib.lookupFunction<_SaveC, _SaveDart>('latex_save_atomic');
    _autosaveWrite =
        _lib.lookupFunction<_SaveC, _SaveDart>('latex_autosave_write');
    _autosaveList =
        _lib.lookupFunction<_ListC, _ListDart>('latex_autosave_list');
    _autosaveRead =
        _lib.lookupFunction<_ReadC, _ReadDart>('latex_autosave_read');
    _autosaveClear =
        _lib.lookupFunction<_ReadC, _ReadDart>('latex_autosave_clear');
    _enginesList = _lib
        .lookupFunction<_EnginesListC, _EnginesListDart>('latex_engines_list');
    _tplList =
        _lib.lookupFunction<_TplListC, _TplListDart>('latex_templates_list');
    _tplCreate =
        _lib.lookupFunction<_TplCreateC, _TplCreateDart>('latex_template_create');
    _publishArchive = _lib
        .lookupFunction<_PublishC, _PublishDart>('latex_publish_archive');
    _verifyProject =
        _lib.lookupFunction<_VerifyC, _VerifyDart>('latex_verify_project');
    _correctPropose = _lib.lookupFunction<_CorrectProposeC, _CorrectProposeDart>(
        'latex_correct_propose');
    _correctApply = _lib.lookupFunction<_CorrectApplyC, _CorrectApplyDart>(
        'latex_correct_apply');
    _free = _lib.lookupFunction<_FreeC, _FreeDart>('latex_free_string');
  }

  static LatexNative open([String? customPath]) {
    if (customPath != null) {
      return LatexNative._(DynamicLibrary.open(customPath));
    }
    final DynamicLibrary lib;
    if (Platform.isAndroid) {
      lib = DynamicLibrary.open('liblatex_ffi.so');
    } else if (Platform.isLinux) {
      lib = _loadWithFallbacks([
        'liblatex_ffi.so',
        '../target/debug/liblatex_ffi.so',
        '../target/release/liblatex_ffi.so',
        'target/debug/liblatex_ffi.so',
        'target/release/liblatex_ffi.so',
      ]);
    } else if (Platform.isMacOS) {
      lib = _loadWithFallbacks([
        'liblatex_ffi.dylib',
        '../target/debug/liblatex_ffi.dylib',
        '../target/release/liblatex_ffi.dylib',
      ]);
    } else if (Platform.isWindows) {
      lib = _loadWithFallbacks([
        'latex_ffi.dll',
        r'..\target\debug\latex_ffi.dll',
        r'..\target\release\latex_ffi.dll',
      ]);
    } else {
      throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
    }
    return LatexNative._(lib);
  }

  static DynamicLibrary _loadWithFallbacks(List<String> candidates) {
    Object? lastError;
    for (final candidate in candidates) {
      try {
        return DynamicLibrary.open(candidate);
      } catch (e) {
        lastError = e;
      }
    }
    throw ArgumentError(
        'Failed to load native library from candidates: $candidates. Last error: $lastError');
  }

  // ---------- internals ------------------------------------------------------

  Map<String, dynamic> _decodeAndFree(Pointer<Utf8> raw) {
    final s = raw.toDartString();
    _free(raw);
    return jsonDecode(s) as Map<String, dynamic>;
  }

  Map<String, dynamic> _three(
      String a, String b, String c, _SaveDart fn) {
    final pa = a.toNativeUtf8();
    final pb = b.toNativeUtf8();
    final pc = c.toNativeUtf8();
    try {
      return _decodeAndFree(fn(pa, pb, pc));
    } finally {
      malloc.free(pa);
      malloc.free(pb);
      malloc.free(pc);
    }
  }

  Map<String, dynamic> _two(String a, String b, _ReadDart fn) {
    final pa = a.toNativeUtf8();
    final pb = b.toNativeUtf8();
    try {
      return _decodeAndFree(fn(pa, pb));
    } finally {
      malloc.free(pa);
      malloc.free(pb);
    }
  }

  Map<String, dynamic> _one(String a, _IndexDart fn) {
    final pa = a.toNativeUtf8();
    try {
      return _decodeAndFree(fn(pa));
    } finally {
      malloc.free(pa);
    }
  }

  // ---------- public API -----------------------------------------------------

  Map<String, dynamic> compile(String engine, String entry, String root, String out) {
    final g = engine.toNativeUtf8();
    final e = entry.toNativeUtf8();
    final r = root.toNativeUtf8();
    final o = out.toNativeUtf8();
    try {
      return _decodeAndFree(_compile(g, e, r, o));
    } finally {
      malloc.free(g);
      malloc.free(e);
      malloc.free(r);
      malloc.free(o);
    }
  }

  Map<String, dynamic> projectIndex(String root) => _one(root, _index);

  Map<String, dynamic> saveAtomic(String root, String rel, String content) =>
      _three(root, rel, content, _saveAtomic);

  Map<String, dynamic> autosaveWrite(String root, String rel, String content) =>
      _three(root, rel, content, _autosaveWrite);

  Map<String, dynamic> autosaveList(String root) => _one(root, _autosaveList);

  Map<String, dynamic> autosaveRead(String root, String rel) =>
      _two(root, rel, _autosaveRead);

  Map<String, dynamic> autosaveClear(String root, String rel) =>
      _two(root, rel, _autosaveClear);

  Map<String, dynamic> enginesList() {
    return _decodeAndFree(_enginesList());
  }

  Map<String, dynamic> templatesList() {
    return _decodeAndFree(_tplList());
  }

  Map<String, dynamic> templateCreate(String name, String targetDir) {
    final n = name.toNativeUtf8();
    final t = targetDir.toNativeUtf8();
    try {
      return _decodeAndFree(_tplCreate(n, t));
    } finally {
      malloc.free(n);
      malloc.free(t);
    }
  }

  Map<String, dynamic> publishArchive(String root, String outZip, String engine) =>
      _three(root, outZip, engine, _publishArchive);

  Map<String, dynamic> verifyProject(String root) => _one(root, _verifyProject);

  Map<String, dynamic> correctPropose(String root, String diagnosticsJson) =>
      _two(root, diagnosticsJson, _correctPropose);

  Map<String, dynamic> correctApply(String root, String correctionJson) =>
      _two(root, correctionJson, _correctApply);
}

// ---------- worker-isolate helpers -------------------------------------------
//
// DynamicLibrary must be opened inside the worker isolate; the handle cannot
// be sent across.

Future<Map<String, dynamic>> compileOffUi(
    String engine, String entry, String root, String out) {
  return Isolate.run(() {
    final n = LatexNative.open();
    return n.compile(engine, entry, root, out);
  });
}

Future<Map<String, dynamic>> indexOffUi(String root) {
  return Isolate.run(() {
    final n = LatexNative.open();
    return n.projectIndex(root);
  });
}

Future<Map<String, dynamic>> verifyOffUi(String root) {
  return Isolate.run(() {
    final n = LatexNative.open();
    return n.verifyProject(root);
  });
}

Future<Map<String, dynamic>> proposeOffUi(String root, String diagnosticsJson) {
  return Isolate.run(() {
    final n = LatexNative.open();
    return n.correctPropose(root, diagnosticsJson);
  });
}
