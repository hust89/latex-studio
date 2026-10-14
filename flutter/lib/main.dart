import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_code_editor/flutter_code_editor.dart';
import 'package:flutter_highlight/themes/atom-one-dark.dart';
import 'package:highlight/languages/tex.dart' as hl_tex;
import 'package:pdfrx/pdfrx.dart';

// ---------- FFI --------------------------------------------------------------

typedef _CompileC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _CompileDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _IndexC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _IndexDart = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _SaveC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _SaveDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
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
typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

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
  late final _FreeDart _free;

  LatexNative._(this._lib) {
    _compile = _lib.lookupFunction<_CompileC, _CompileDart>('latex_compile');
    _index = _lib.lookupFunction<_IndexC, _IndexDart>('latex_project_index');
    _saveAtomic = _lib.lookupFunction<_SaveC, _SaveDart>('latex_save_atomic');
    _autosaveWrite = _lib.lookupFunction<_SaveC, _SaveDart>('latex_autosave_write');
    _autosaveList = _lib.lookupFunction<_ListC, _ListDart>('latex_autosave_list');
    _autosaveRead = _lib.lookupFunction<_ReadC, _ReadDart>('latex_autosave_read');
    _autosaveClear = _lib.lookupFunction<_ReadC, _ReadDart>('latex_autosave_clear');
    _enginesList = _lib.lookupFunction<_EnginesListC, _EnginesListDart>('latex_engines_list');
    _tplList = _lib.lookupFunction<_TplListC, _TplListDart>('latex_templates_list');
    _tplCreate = _lib.lookupFunction<_TplCreateC, _TplCreateDart>('latex_template_create');
    _free = _lib.lookupFunction<_FreeC, _FreeDart>('latex_free_string');
  }

  static LatexNative open() {
    final DynamicLibrary lib;
    if (Platform.isAndroid || Platform.isLinux) {
      lib = DynamicLibrary.open('liblatex_ffi.so');
    } else if (Platform.isMacOS) {
      lib = DynamicLibrary.open('liblatex_ffi.dylib');
    } else if (Platform.isWindows) {
      lib = DynamicLibrary.open('latex_ffi.dll');
    } else {
      throw UnsupportedError('Unsupported platform');
    }
    return LatexNative._(lib);
  }

  Map<String, dynamic> _decodeAndFree(Pointer<Utf8> raw) {
    final s = raw.toDartString();
    _free(raw);
    return jsonDecode(s) as Map<String, dynamic>;
  }

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

  Map<String, dynamic> index(String root) {
    final r = root.toNativeUtf8();
    try {
      return _decodeAndFree(_index(r));
    } finally {
      malloc.free(r);
    }
  }

  Map<String, dynamic> _three(String root, String rel, String content, _SaveDart fn) {
    final a = root.toNativeUtf8();
    final b = rel.toNativeUtf8();
    final c = content.toNativeUtf8();
    try {
      return _decodeAndFree(fn(a, b, c));
    } finally {
      malloc.free(a);
      malloc.free(b);
      malloc.free(c);
    }
  }

  Map<String, dynamic> saveAtomic(String root, String rel, String content) =>
      _three(root, rel, content, _saveAtomic);

  Map<String, dynamic> autosaveWrite(String root, String rel, String content) =>
      _three(root, rel, content, _autosaveWrite);

  Map<String, dynamic> _two(String root, String rel, _ReadDart fn) {
    final a = root.toNativeUtf8();
    final b = rel.toNativeUtf8();
    try {
      return _decodeAndFree(fn(a, b));
    } finally {
      malloc.free(a);
      malloc.free(b);
    }
  }

  Map<String, dynamic> autosaveRead(String root, String rel) =>
      _two(root, rel, _autosaveRead);

  Map<String, dynamic> autosaveClear(String root, String rel) =>
      _two(root, rel, _autosaveClear);

  Map<String, dynamic> autosaveList(String root) {
    final r = root.toNativeUtf8();
    try {
      return _decodeAndFree(_autosaveList(r));
    } finally {
      malloc.free(r);
    }
  }

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
}

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
    return n.index(root);
  });
}

// ---------- Model ----------------------------------------------------------

class ProjectFile {
  final String path;
  final String kind;
  ProjectFile(this.path, this.kind);
}

class ProjectIndex {
  final List<ProjectFile> files;
  final String? entry;
  ProjectIndex(this.files, this.entry);
}

class Recoverable {
  final String path;
  final int autosaveMtimeMs;
  final int originalMtimeMs;
  Recoverable(this.path, this.autosaveMtimeMs, this.originalMtimeMs);
}

// ---------- App --------------------------------------------------------------

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  pdfrxFlutterInitialize();
  runApp(const LatexApp());
}

class LatexApp extends StatelessWidget {
  const LatexApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'LaTeX Studio',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.indigo,
          brightness: Brightness.dark,
        ),
      ),
      home: const HomePage(),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  static const Duration _autosaveDebounce = Duration(seconds: 2);

  final TextEditingController _pathController = TextEditingController();

  LatexNative? _native;
  String? _nativeError;

  String? _projectRoot;
  ProjectIndex? _projectIndex;
  String? _entry;
  String? _openFile;
  String? _openFileAbs;
  CodeController? _editor;
  bool _dirty = false;
  Map<String, dynamic>? _result;
  bool _busy = false;
  Timer? _autosaveTimer;
  String _engineId = 'tectonic';
  List<Map<String, dynamic>> _engines = const [];

  @override
  void initState() {
    super.initState();
    try {
      _native = LatexNative.open();
      _refreshEngines();
    } catch (e) {
      _nativeError = 'native library not found: $e';
    }
  }

  @override
  void dispose() {
    _autosaveTimer?.cancel();
    _pathController.dispose();
    _editor?.dispose();
    super.dispose();
  }

  void _refreshEngines() {
    final native = _native;
    if (native == null) return;
    try {
      final res = native.enginesList();
      final list = (res['engines'] as List?) ?? const [];
      final def = res['default'] as String?;
      if (!mounted) return;
      setState(() {
        _engines = list.cast<Map<String, dynamic>>();
        if (def != null && _engines.any((e) => e['id'] == def)) {
          _engineId = def;
        }
      });
    } catch (_) {}
  }

  Future<void> _pickProject() async {
    final dir = await FilePicker.platform.getDirectoryPath();
    if (dir == null) return;
    await _loadRoot(dir);
  }

  Future<void> _loadByPath() async {
    final raw = _pathController.text.trim();
    if (raw.isEmpty) return;
    final expanded = raw.startsWith('~')
        ? raw.replaceFirst('~', Platform.environment['HOME'] ?? '')
        : raw;
    await _loadRoot(expanded);
  }

  Future<void> _loadRoot(String root) async {
    if (!Directory(root).existsSync()) {
      setState(() =>
          _result = <String, dynamic>{'error': 'not a directory: $root'});
      return;
    }

    setState(() => _busy = true);

    Map<String, dynamic> idx;
    try {
      idx = await indexOffUi(root);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _result = <String, dynamic>{'error': 'index failed: $e'};
      });
      return;
    }

    if (!mounted) return;
    if (idx['error'] != null) {
      setState(() {
        _busy = false;
        _result = <String, dynamic>{'error': idx['error']};
      });
      return;
    }

    final rawFiles = (idx['files'] as List?) ?? const [];
    final files = rawFiles
        .map((f) => ProjectFile(
              (f as Map)['path'] as String,
              f['kind'] as String,
            ))
        .toList();
    final entry = idx['entry'] as String?;

    _editor?.dispose();
    _editor = null;

    setState(() {
      _busy = false;
      _projectRoot = root;
      _projectIndex = ProjectIndex(files, entry);
      _entry = entry;
      _result = null;
      _pathController.text = root;
      _openFile = null;
      _openFileAbs = null;
      _dirty = false;
    });

    await _offerRecovery(root);

    if (entry != null) {
      await _openProjectFile(entry);
    }
  }

  Future<void> _newFromTemplate() async {
    final native = _native;
    if (native == null) return;

    List<Map<String, String>> templates;
    try {
      final res = native.templatesList();
      final raw = (res['templates'] as List?) ?? const [];
      templates = raw
          .map((t) => {
                'name': (t as Map)['name'].toString(),
                'description': t['description'].toString(),
              })
          .toList();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('templates: $e')));
      return;
    }
    if (templates.isEmpty || !mounted) return;

    final result = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) => _TemplateDialog(templates: templates),
    );
    if (result == null || !mounted) return;

    final created = native.templateCreate(result['template']!, result['path']!);
    if (created['error'] != null) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Create failed: ${created['error']}')),
      );
      return;
    }

    final root = created['path'] as String? ?? result['path']!;
    await _loadRoot(root);
  }

  Future<void> _offerRecovery(String root) async {
    final native = _native;
    if (native == null) return;

    List<Recoverable> recoverables;
    try {
      final res = native.autosaveList(root);
      if (res['error'] != null) return;
      final rawList = (res['recoverable'] as List?) ?? const [];
      recoverables = rawList
          .map((r) => Recoverable(
                (r as Map)['path'] as String,
                (r['autosave_mtime_ms'] as num).toInt(),
                (r['original_mtime_ms'] as num).toInt(),
              ))
          .toList();
    } catch (_) {
      return;
    }
    if (recoverables.isEmpty || !mounted) return;

    final choice = await showDialog<String>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) => AlertDialog(
        title: const Text('Unsaved work found'),
        content: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'The following files have autosaves newer than what is on '
                'disk. Recover them or discard the autosaves?',
              ),
              const SizedBox(height: 12),
              for (final r in recoverables)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 2),
                  child: Text(
                    '${r.path}  (autosaved ${_fmt(r.autosaveMtimeMs)})',
                    style:
                        const TextStyle(fontFamily: 'monospace', fontSize: 12),
                  ),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop('discard'),
            child: const Text('Discard'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop('recover'),
            child: const Text('Recover'),
          ),
        ],
      ),
    );

    if (choice == 'recover') {
      for (final r in recoverables) {
        try {
          final res = native.autosaveRead(root, r.path);
          if (res['error'] != null) continue;
          final content = res['content'] as String? ?? '';
          native.saveAtomic(root, r.path, content);
        } catch (_) {}
      }
    } else {
      for (final r in recoverables) {
        try {
          native.autosaveClear(root, r.path);
        } catch (_) {}
      }
    }
  }

  static String _fmt(int ms) {
    final d = DateTime.fromMillisecondsSinceEpoch(ms);
    final hh = d.hour.toString().padLeft(2, '0');
    final mm = d.minute.toString().padLeft(2, '0');
    return '$hh:$mm';
  }

  Future<void> _openProjectFile(String rel) async {
    final root = _projectRoot;
    if (root == null) return;

    if (_dirty) {
      await _save();
    }

    final abs = '$root/$rel';
    final f = File(abs);
    if (!f.existsSync()) return;

    final content = await f.readAsString();
    final controller = CodeController(text: content, language: hl_tex.tex);
    controller.addListener(_onEditorChanged);

    _editor?.dispose();
    setState(() {
      _openFile = rel;
      _openFileAbs = abs;
      _editor = controller;
      _dirty = false;
    });
  }

  void _onEditorChanged() {
    if (!_dirty && mounted) {
      setState(() => _dirty = true);
    }
    _autosaveTimer?.cancel();
    _autosaveTimer = Timer(_autosaveDebounce, _autosaveNow);
  }

  void _autosaveNow() {
    final native = _native;
    final root = _projectRoot;
    final rel = _openFile;
    final editor = _editor;
    if (native == null || root == null || rel == null || editor == null) return;
    if (!_dirty) return;
    try {
      native.autosaveWrite(root, rel, editor.text);
    } catch (_) {}
  }

  Future<void> _save() async {
    final native = _native;
    final root = _projectRoot;
    final rel = _openFile;
    final editor = _editor;
    if (native == null || root == null || rel == null || editor == null) return;

    _autosaveTimer?.cancel();
    try {
      final res = native.saveAtomic(root, rel, editor.text);
      if (res['error'] != null) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Save failed: ${res['error']}')),
        );
        return;
      }
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Save failed: $e')),
      );
      return;
    }

    if (!mounted) return;
    setState(() => _dirty = false);
  }

  Future<void> _compile() async {
    final entry = _entry;
    final root = _projectRoot;
    if (entry == null || root == null) return;

    if (_dirty) {
      await _save();
    }

    final outParent = '$root/.latex-studio-build';
    Directory(outParent).createSync(recursive: true);

    setState(() => _busy = true);

    Map<String, dynamic> r;
    try {
      r = await compileOffUi(_engineId, '$root/$entry', root, outParent);
    } catch (e) {
      r = <String, dynamic>{'error': 'compile failure: $e'};
    }

    if (!mounted) return;
    setState(() {
      _result = r;
      _busy = false;
    });
  }

  Widget _fileTree() {
    final idx = _projectIndex;
    if (idx == null) {
      return const Center(child: Text('No project loaded'));
    }

    final byDir = <String, List<ProjectFile>>{};
    for (final f in idx.files) {
      final slash = f.path.lastIndexOf('/');
      final dir = slash < 0 ? '' : f.path.substring(0, slash);
      byDir.putIfAbsent(dir, () => <ProjectFile>[]).add(f);
    }
    final dirs = byDir.keys.toList()..sort();

    final rows = <Widget>[];
    for (final d in dirs) {
      if (d.isNotEmpty) {
        rows.add(Padding(
          padding: const EdgeInsets.fromLTRB(12, 12, 8, 4),
          child: Text(
            d,
            style: const TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.bold,
              letterSpacing: 0.5,
            ),
          ),
        ));
      }
      final files = byDir[d]!..sort((a, b) => a.path.compareTo(b.path));
      for (final f in files) {
        rows.add(_fileRow(f, idx.entry == f.path));
      }
    }

    return ListView(children: rows);
  }

  Widget _fileRow(ProjectFile f, bool isEntry) {
    final selected = _openFile == f.path;
    final name = f.path.split('/').last;

    IconData icon;
    switch (f.kind) {
      case 'Source':
        icon = Icons.description_outlined;
        break;
      case 'Bibliography':
        icon = Icons.library_books_outlined;
        break;
      case 'Style':
        icon = Icons.brush_outlined;
        break;
      case 'Asset':
        icon = Icons.image_outlined;
        break;
      default:
        icon = Icons.insert_drive_file_outlined;
    }

    return ListTile(
      dense: true,
      visualDensity: VisualDensity.compact,
      selected: selected,
      leading: Icon(icon, size: 16),
      title: Row(
        children: [
          Expanded(
            child: Text(
              name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 13,
                fontWeight: isEntry ? FontWeight.bold : FontWeight.normal,
              ),
            ),
          ),
          if (isEntry)
            const Icon(Icons.star, size: 12, color: Colors.amber),
        ],
      ),
      onTap: () => _openProjectFile(f.path),
    );
  }

  Widget _editorPane() {
    final editor = _editor;
    if (editor == null) {
      return const Center(child: Text('Select a file to edit'));
    }
    return CodeTheme(
      data: CodeThemeData(styles: atomOneDarkTheme),
      child: Container(
        color: const Color(0xFF282C34),
        child: SingleChildScrollView(
          child: CodeField(
            controller: editor,
            textStyle: const TextStyle(
              fontFamily: 'monospace',
              fontSize: 13,
              height: 1.45,
            ),
            gutterStyle: const GutterStyle(
              showLineNumbers: true,
              showErrors: false,
              showFoldingHandles: false,
            ),
          ),
        ),
      ),
    );
  }

  Widget _pdfPane() {
    final r = _result;
    if (r == null) return const Center(child: Text('No build yet'));
    if (r['success'] != true) {
      return const Center(child: Text('No fresh PDF'));
    }
    final pdf = r['pdf_path'] as String?;
    if (pdf == null || !File(pdf).existsSync()) {
      return const Center(child: Text('PDF path missing'));
    }
    return PdfViewer.file(pdf);
  }

  Widget _diagnosticsPane() {
    final r = _result;
    if (r == null) return const Center(child: Text('No build yet'));
    if (r['error'] != null) {
      return Padding(
        padding: const EdgeInsets.all(12),
        child: Text(
          r['error'].toString(),
          style: const TextStyle(color: Colors.red, fontSize: 13),
        ),
      );
    }
    final diags = (r['diagnostics'] as List?)?.cast<Map>() ?? const <Map>[];
    if (diags.isEmpty) {
      return Center(
        child: Text(
          r['success'] == true
              ? 'Build succeeded - no diagnostics'
              : 'Build failed - no parsed diagnostics',
          style: const TextStyle(fontSize: 13),
        ),
      );
    }
    return ListView.builder(
      itemCount: diags.length,
      itemBuilder: (_, i) {
        final d = diags[i];
        final sev = d['severity']?.toString() ?? '?';
        final loc = d['location'] as Map?;
        final locText = loc == null
            ? ''
            : '${loc['file']}:${loc['line']}'
                '${loc['column'] != null ? ':${loc['column']}' : ''}';
        return ListTile(
          dense: true,
          visualDensity: VisualDensity.compact,
          leading: Icon(
            sev == 'Error' ? Icons.error_outline : Icons.warning_amber,
            color: sev == 'Error' ? Colors.red : Colors.orange,
            size: 18,
          ),
          title: Text(
            d['message']?.toString() ?? '',
            style: const TextStyle(fontSize: 13),
          ),
          subtitle: locText.isEmpty
              ? null
              : Text(locText, style: const TextStyle(fontSize: 11)),
        );
      },
    );
  }

  Widget _editorAndPdfTabs() {
    return DefaultTabController(
      length: 2,
      child: Column(
        children: [
          const TabBar(tabs: [Tab(text: 'Editor'), Tab(text: 'PDF')]),
          Expanded(
            child: TabBarView(children: [_editorPane(), _pdfPane()]),
          ),
        ],
      ),
    );
  }

  Widget _wide() {
    return Row(
      children: [
        SizedBox(width: 220, child: _fileTree()),
        const VerticalDivider(width: 1),
        Expanded(flex: 3, child: _editorAndPdfTabs()),
        const VerticalDivider(width: 1),
        SizedBox(width: 340, child: _diagnosticsPane()),
      ],
    );
  }

  Widget _medium() {
    return Column(
      children: [
        Expanded(
          child: Row(
            children: [
              SizedBox(width: 200, child: _fileTree()),
              const VerticalDivider(width: 1),
              Expanded(child: _editorAndPdfTabs()),
            ],
          ),
        ),
        const Divider(height: 1),
        SizedBox(height: 160, child: _diagnosticsPane()),
      ],
    );
  }

  Widget _narrow() {
    return DefaultTabController(
      length: 4,
      child: Column(
        children: [
          const TabBar(
            tabs: [
              Tab(text: 'Files'),
              Tab(text: 'Editor'),
              Tab(text: 'PDF'),
              Tab(text: 'Problems'),
            ],
          ),
          Expanded(
            child: TabBarView(
              children: [
                _fileTree(),
                _editorPane(),
                _pdfPane(),
                _diagnosticsPane(),
              ],
            ),
          ),
        ],
      ),
    );
  }

  String _windowTitle() {
    final buf = StringBuffer('LaTeX Studio');
    if (_projectRoot != null) {
      final sep = Platform.pathSeparator;
      final parts = _projectRoot!.split(sep).where((s) => s.isNotEmpty).toList();
      final name = parts.isEmpty ? _projectRoot : parts.last;
      buf.write(' \u2014 $name');
      if (_openFile != null) {
        buf.write(' / ${_openFile!.split('/').last}');
      }
      if (_dirty) buf.write(' \u2022');
    }
    return buf.toString();
  }

  @override
  Widget build(BuildContext context) {
    final r = _result;

    return Shortcuts(
      shortcuts: <ShortcutActivator, Intent>{
        const SingleActivator(LogicalKeyboardKey.keyS, control: true):
            const _SaveIntent(),
      },
      child: Actions(
        actions: <Type, Action<Intent>>{
          _SaveIntent: CallbackAction<_SaveIntent>(
            onInvoke: (_) {
              _save();
              return null;
            },
          ),
        },
        child: Focus(
          autofocus: true,
          child: Scaffold(
            appBar: AppBar(
              title: Text(_windowTitle(), overflow: TextOverflow.ellipsis),
              actions: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  child: DropdownButton<String>(
                    value: _engineId,
                    underline: const SizedBox.shrink(),
                    onChanged: _busy
                        ? null
                        : (v) {
                            if (v != null) setState(() => _engineId = v);
                          },
                    items: _engines
                        .map((e) => DropdownMenuItem<String>(
                              value: e['id'] as String,
                              enabled: e['available'] == true,
                              child: Text(
                                e['available'] == true
                                    ? (e['display_name'] as String)
                                    : '${e['display_name']} (unavailable)',
                              ),
                            ))
                        .toList(),
                  ),
                ),
                if (_busy)
                  const Padding(
                    padding: EdgeInsets.symmetric(horizontal: 16),
                    child: Center(
                      child: SizedBox(
                        width: 20,
                        height: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                    ),
                  ),
                IconButton(
                  onPressed: _busy ? null : _newFromTemplate,
                  icon: const Icon(Icons.note_add_outlined),
                  tooltip: 'New from template',
                ),
                IconButton(
                  onPressed: _busy ? null : _pickProject,
                  icon: const Icon(Icons.folder_open),
                  tooltip: 'Open project',
                ),
                IconButton(
                  onPressed: (_editor == null || _busy) ? null : _save,
                  icon: const Icon(Icons.save_outlined),
                  tooltip: 'Save (Ctrl+S)',
                ),
                IconButton(
                  onPressed: (_entry == null || _busy) ? null : _compile,
                  icon: const Icon(Icons.play_arrow),
                  tooltip: 'Compile',
                ),
              ],
            ),
            body: Column(
              children: [
                if (_nativeError != null)
                  MaterialBanner(
                    content: Text(_nativeError!),
                    actions: const [SizedBox.shrink()],
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(8, 8, 8, 4),
                  child: Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _pathController,
                          decoration: const InputDecoration(
                            labelText: 'Project path',
                            isDense: true,
                            border: OutlineInputBorder(),
                          ),
                          onSubmitted: (_) => _loadByPath(),
                        ),
                      ),
                      const SizedBox(width: 8),
                      FilledButton(
                        onPressed: _busy ? null : _loadByPath,
                        child: const Text('Load'),
                      ),
                    ],
                  ),
                ),
                if (r != null && r['error'] != null)
                  MaterialBanner(
                    content: Text(r['error'].toString()),
                    actions: const [SizedBox.shrink()],
                  ),
                Expanded(
                  child: LayoutBuilder(
                    builder: (_, c) {
                      if (c.maxWidth >= 1100) return _wide();
                      if (c.maxWidth >= 700) return _medium();
                      return _narrow();
                    },
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _SaveIntent extends Intent {
  const _SaveIntent();
}

class _TemplateDialog extends StatefulWidget {
  final List<Map<String, String>> templates;
  const _TemplateDialog({required this.templates});

  @override
  State<_TemplateDialog> createState() => _TemplateDialogState();
}

class _TemplateDialogState extends State<_TemplateDialog> {
  late String _chosen = widget.templates.first['name']!;
  final TextEditingController _path = TextEditingController();

  @override
  void initState() {
    super.initState();
    final home = Platform.environment['HOME'] ?? '';
    _path.text = home.isEmpty ? '' : '$home/newproject';
  }

  @override
  void dispose() {
    _path.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('New from template'),
      content: SizedBox(
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Template'),
            const SizedBox(height: 4),
            DropdownButtonFormField<String>(
              initialValue: _chosen,
              items: widget.templates
                  .map((t) => DropdownMenuItem<String>(
                        value: t['name'],
                        child: Text('${t['name']}  -  ${t['description']}'),
                      ))
                  .toList(),
              onChanged: (v) {
                if (v != null) setState(() => _chosen = v);
              },
            ),
            const SizedBox(height: 16),
            const Text('Target directory'),
            const SizedBox(height: 4),
            TextField(
              controller: _path,
              decoration: const InputDecoration(
                hintText: '/home/you/newproject',
                border: OutlineInputBorder(),
                isDense: true,
              ),
            ),
            const SizedBox(height: 8),
            const Text(
              'The directory must not exist or must be empty.',
              style: TextStyle(fontSize: 11),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () {
            final p = _path.text.trim();
            if (p.isEmpty) return;
            Navigator.of(context).pop({'template': _chosen, 'path': p});
          },
          child: const Text('Create'),
        ),
      ],
    );
  }
}
