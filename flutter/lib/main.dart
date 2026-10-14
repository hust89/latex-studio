import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:pdfrx/pdfrx.dart';

// ---------- FFI binding ------------------------------------------------------

typedef _CompileC = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _CompileDart = Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>);
typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

class LatexNative {
  final DynamicLibrary _lib;
  late final _CompileDart _compile;
  late final _FreeDart _free;

  LatexNative._(this._lib) {
    _compile = _lib.lookupFunction<_CompileC, _CompileDart>('latex_compile');
    _free    = _lib.lookupFunction<_FreeC, _FreeDart>('latex_free_string');
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

  Map<String, dynamic> compile(String entry, String projectRoot, String outParent) {
    final e = entry.toNativeUtf8();
    final r = projectRoot.toNativeUtf8();
    final o = outParent.toNativeUtf8();
    try {
      final raw = _compile(e, r, o);
      final s = raw.toDartString();
      _free(raw);
      return jsonDecode(s) as Map<String, dynamic>;
    } finally {
      malloc.free(e);
      malloc.free(r);
      malloc.free(o);
    }
  }
}

// Runs on a worker isolate so the UI isolate stays responsive during long
// compilations. DynamicLibrary must be opened inside the worker; it cannot
// be captured across isolates.
Future<Map<String, dynamic>> compileOffUi(
  String entry,
  String projectRoot,
  String outParent,
) {
  return Isolate.run(() {
    final native = LatexNative.open();
    return native.compile(entry, projectRoot, outParent);
  });
}

// ---------- UI ---------------------------------------------------------------

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
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
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
  String? _projectRoot;
  String? _entry;
  final TextEditingController _pathController = TextEditingController();
  Map<String, dynamic>? _result;
  bool _busy = false;

  Future<void> _pickProject() async {
    final dir = await FilePicker.platform.getDirectoryPath();
    if (dir == null) return;
    final tex = Directory(dir)
        .listSync()
        .whereType<File>()
        .where((f) => f.path.endsWith('.tex'))
        .map((f) => f.path)
        .toList();
    tex.sort();
    setState(() {
      _projectRoot = dir;
      _entry = tex.isEmpty ? null : tex.first;
      _result = null;
    });
  }

  void _loadByPath() {
    final raw = _pathController.text.trim();
    if (raw.isEmpty) return;
    final expanded = raw.startsWith('~')
        ? raw.replaceFirst('~', Platform.environment['HOME'] ?? '')
        : raw;
    final d = Directory(expanded);
    if (!d.existsSync()) {
      setState(() {
        _result = <String, dynamic>{
          'error': 'not a directory: \$expanded',
        };
      });
      return;
    }
    final tex = d
        .listSync()
        .whereType<File>()
        .where((f) => f.path.endsWith('.tex'))
        .map((f) => f.path)
        .toList();
    tex.sort();
    setState(() {
      _projectRoot = expanded;
      _entry = tex.isEmpty ? null : tex.first;
      _result = null;
    });
  }

  @override
  void dispose() {
    _pathController.dispose();
    super.dispose();
  }

  Future<void> _compile() async {
    final entry = _entry;
    final root = _projectRoot;
    if (entry == null || root == null) return;

    final outParent = '$root/.latex-studio-build';
    Directory(outParent).createSync(recursive: true);

    setState(() {
      _busy = true;
    });

    Map<String, dynamic> r;
    try {
      r = await compileOffUi(entry, root, outParent);
    } catch (e) {
      r = <String, dynamic>{'error': 'isolate failure: $e'};
    }

    if (!mounted) return;
    setState(() {
      _result = r;
      _busy = false;
    });
  }

  Widget _diagnosticsPane() {
    final r = _result;
    if (r == null) {
      return const Center(child: Text('No build yet'));
    }
    if (r['error'] != null) {
      return Padding(
        padding: const EdgeInsets.all(16),
        child: Text(
          r['error'].toString(),
          style: const TextStyle(color: Colors.red),
        ),
      );
    }
    final diags = (r['diagnostics'] as List?)?.cast<Map>() ?? const <Map>[];
    if (diags.isEmpty) {
      return Center(
        child: Text(r['success'] == true
            ? 'Build succeeded - no diagnostics'
            : 'Build failed - no parsed diagnostics'),
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
          leading: Icon(
            sev == 'Error' ? Icons.error_outline : Icons.warning_amber,
            color: sev == 'Error' ? Colors.red : Colors.orange,
          ),
          title: Text(d['message']?.toString() ?? ''),
          subtitle: locText.isEmpty ? null : Text(locText),
        );
      },
    );
  }

  Widget _pdfPane() {
    final r = _result;
    if (r == null) {
      return const Center(child: Text('No PDF yet'));
    }
    if (r['success'] != true) {
      return const Center(child: Text('Build failed - no fresh PDF produced'));
    }
    final pdf = r['pdf_path'] as String?;
    if (pdf == null || !File(pdf).existsSync()) {
      return const Center(child: Text('PDF path missing'));
    }
    return PdfViewer.file(pdf);
  }

  Widget _wide() {
    return Row(
      children: [
        SizedBox(width: 380, child: _diagnosticsPane()),
        const VerticalDivider(width: 1),
        Expanded(child: _pdfPane()),
      ],
    );
  }

  Widget _narrow() {
    return DefaultTabController(
      length: 2,
      child: Column(
        children: [
          const TabBar(tabs: [Tab(text: 'Diagnostics'), Tab(text: 'PDF')]),
          Expanded(
            child: TabBarView(
              children: [_diagnosticsPane(), _pdfPane()],
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final r = _result;
    final sep = Platform.pathSeparator;
    final title = _projectRoot == null
        ? 'LaTeX Studio'
        : '${_projectRoot!.split(sep).last}'
            '${_entry == null ? '' : ' - ${_entry!.split(sep).last}'}';

    return Scaffold(
      appBar: AppBar(
        title: Text(title, overflow: TextOverflow.ellipsis),
        actions: [
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
            onPressed: _busy ? null : _pickProject,
            icon: const Icon(Icons.folder_open),
            tooltip: 'Open project',
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
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _pathController,
                    decoration: const InputDecoration(
                      labelText: 'Project path',
                      hintText: '/home/hust89/latex-studio/corpus/valid-minimal',
                      border: OutlineInputBorder(),
                    ),
                    onSubmitted: (_) => _loadByPath(),
                  ),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: _loadByPath,
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
              builder: (_, c) => c.maxWidth >= 900 ? _wide() : _narrow(),
            ),
          ),
        ],
      ),
    );
  }
}
