import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_code_editor/flutter_code_editor.dart';
import 'package:flutter_highlight/themes/atom-one-dark.dart';
import 'package:highlight/languages/tex.dart' as hl_tex;
import 'package:pdfrx/pdfrx.dart';

import 'correction_card.dart';
import 'models.dart';
import 'native.dart';

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
  CodeController? _editor;
  bool _dirty = false;
  Map<String, dynamic>? _result;
  bool _busy = false;
  Timer? _autosaveTimer;
  String _engineId = 'tectonic';
  List<Map<String, dynamic>> _engines = const [];

  // Corrections shown in the Problems pane.
  List<Correction> _corrections = const [];
  final Set<String> _rejectedKeys = <String>{};
  final Set<String> _applyingKeys = <String>{};

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

  // ---------- project loading ------------------------------------------------

  Future<void> _pickProject() async {
    String? dir;
    try {
      dir = await FilePicker.platform.getDirectoryPath();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('folder picker failed: $e')),
      );
      return;
    }
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
    // No Dart-side pre-check. Directory.existsSync() throws
    // FileSystemException on scoped-storage paths, even for the app's
    // own directory; the FFI index call already handles path errors
    // and reports them via idx['error'].
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
      _dirty = false;
    });

    await _offerRecovery(root);

    if (entry != null) {
      await _openProjectFile(entry);
    }
    await _refreshCorrections();
  }

  // ---------- new from template ---------------------------------------------

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

  // ---------- recovery -------------------------------------------------------

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

  // ---------- editing --------------------------------------------------------

  // ---------- corrections ----------------------------------------------------

  String _corrKey(Correction c) =>
      '${c.file}:${c.line}:${c.oldText}=>${c.newText}';

  Future<void> _refreshCorrections() async {
    final root = _projectRoot;
    if (root == null) return;
    try {
      final verifyRes = await verifyOffUi(root);
      if (verifyRes['error'] != null) return;
      final diagsJson = jsonEncode(verifyRes['diagnostics'] ?? const []);
      final proposeRes = await proposeOffUi(root, diagsJson);
      if (proposeRes['error'] != null) return;
      final rawCorrs = (proposeRes['corrections'] as List?) ?? const [];
      final corrections = rawCorrs
          .map((c) => Correction.fromJson((c as Map).cast<String, dynamic>()))
          .where((c) => !_rejectedKeys.contains(_corrKey(c)))
          .toList();
      if (!mounted) return;
      setState(() => _corrections = corrections);
    } catch (_) {}
  }

  Future<void> _reloadOpenFile() async {
    final root = _projectRoot;
    final rel = _openFile;
    if (root == null || rel == null) return;
    final abs = '$root/$rel';
    String content;
    try {
      content = await File(abs).readAsString();
    } on FileSystemException {
      return;
    }
    final controller = CodeController(text: content, language: hl_tex.tex);
    controller.addListener(_onEditorChanged);
    _editor?.dispose();
    if (!mounted) {
      controller.dispose();
      return;
    }
    setState(() {
      _editor = controller;
      _dirty = false;
    });
  }

  Future<void> _applyCorrection(Correction c) async {
    final native = _native;
    final root = _projectRoot;
    if (native == null || root == null) return;
    if (_dirty && c.file == _openFile) {
      await _save();
    }
    final key = _corrKey(c);
    setState(() => _applyingKeys.add(key));
    Map<String, dynamic> res;
    try {
      res = native.correctApply(root, jsonEncode(c.toJson()));
    } catch (e) {
      res = <String, dynamic>{'error': '$e', 'reason': 'io'};
    }
    if (!mounted) return;
    setState(() => _applyingKeys.remove(key));
    if (res['ok'] != true) {
      final msg = res['error']?.toString() ?? 'unknown error';
      final reason = res['reason']?.toString() ?? '';
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Could not apply correction: $msg')),
      );
      if (reason == 'missing' || reason == 'ambiguous' || reason == 'io') {
        await _refreshCorrections();
      }
      return;
    }
    if (c.file == _openFile) {
      await _reloadOpenFile();
    }
    await _refreshCorrections();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Applied: ${c.file}:${c.line}')),
    );
  }

  void _rejectCorrection(Correction c) {
    final key = _corrKey(c);
    setState(() {
      _rejectedKeys.add(key);
      _corrections = _corrections.where((x) => _corrKey(x) != key).toList();
    });
  }

  Future<void> _openProjectFile(String rel) async {
    final root = _projectRoot;
    if (root == null) return;

    if (_dirty) {
      await _save();
    }

    final abs = '$root/$rel';
    String content;
    try {
      content = await File(abs).readAsString();
    } on FileSystemException catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('cannot read $abs: $e')),
      );
      return;
    }
    final controller = CodeController(text: content, language: hl_tex.tex);
    controller.addListener(_onEditorChanged);

    _editor?.dispose();
    setState(() {
      _openFile = rel;
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

  // ---------- compile --------------------------------------------------------

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
    await _refreshCorrections();
  }

  // ---------- publish --------------------------------------------------------

  Future<void> _publish() async {
    final native = _native;
    final root = _projectRoot;
    if (native == null || root == null) return;

    final name = root.split(Platform.pathSeparator).last;
    final suggested = '$name.zip';
    final out = await FilePicker.platform.saveFile(
      dialogTitle: 'Publish archive',
      fileName: suggested,
      type: FileType.custom,
      allowedExtensions: ['zip'],
    );
    if (out == null) return;

    Map<String, dynamic> r;
    try {
      r = native.publishArchive(root, out, _engineId);
    } catch (e) {
      r = <String, dynamic>{'error': 'publish failure: $e'};
    }

    if (!mounted) return;
    if (r['error'] != null) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Publish failed: ${r['error']}')),
      );
      return;
    }
    final files = r['file_count'];
    final pdf = r['pdf_included'] == true ? 'with PDF' : 'sources only';
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Published $files files ($pdf) to $out')),
    );
  }

  // ---------- file tree ------------------------------------------------------

  Widget _fileTree() {
    final idx = _projectIndex;
    if (idx == null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.folder_open, size: 36, color: Colors.grey),
              const SizedBox(height: 8),
              const Text('No project loaded',
                  style: TextStyle(color: Colors.grey)),
              const SizedBox(height: 12),
              FilledButton.tonal(
                onPressed: _busy ? null : _pickProject,
                child: const Text('Open Project'),
              ),
            ],
          ),
        ),
      );
    }

    final byDir = <String, List<ProjectFile>>{};
    for (final f in idx.files) {
      final slash = f.path.lastIndexOf('/');
      final dir = slash < 0 ? '' : f.path.substring(0, slash);
      byDir.putIfAbsent(dir, () => <ProjectFile>[]).add(f);
    }
    final dirs = byDir.keys.toList()..sort();

    final rows = <Widget>[
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 8, 4, 4),
        child: Row(
          children: [
            const Expanded(
              child: Text(
                'PROJECT FILES',
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.bold,
                  letterSpacing: 0.5,
                  color: Colors.white70,
                ),
              ),
            ),
            IconButton(
              icon: const Icon(Icons.note_add_outlined, size: 18),
              tooltip: 'New file',
              visualDensity: VisualDensity.compact,
              onPressed: _busy ? null : _newProjectFile,
            ),
            IconButton(
              icon: const Icon(Icons.refresh, size: 18),
              tooltip: 'Refresh files',
              visualDensity: VisualDensity.compact,
              onPressed: _busy || _projectRoot == null
                  ? null
                  : () => _loadRoot(_projectRoot!),
            ),
          ],
        ),
      ),
      const Divider(height: 1),
    ];

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

  Future<void> _newProjectFile() async {
    final root = _projectRoot;
    if (root == null) return;
    final nameController = TextEditingController(text: 'section.tex');
    final filename = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('New File in Project'),
        content: TextField(
          controller: nameController,
          autofocus: true,
          decoration: const InputDecoration(
            labelText: 'File name (e.g. section.tex)',
            border: OutlineInputBorder(),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(nameController.text.trim()),
            child: const Text('Create'),
          ),
        ],
      ),
    );
    if (filename == null || filename.isEmpty) return;
    final native = _native;
    if (native != null) {
      native.saveAtomic(root, filename, '% $filename\n');
    } else {
      final f = File('$root/$filename');
      if (!f.existsSync()) {
        f.createSync(recursive: true);
        f.writeAsStringSync('% $filename\n');
      }
    }
    await _loadRoot(root);
    await _openProjectFile(filename);
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

  // ---------- editor pane ----------------------------------------------------

  Widget _snippetBar() {
    final chips = <Map<String, String>>[
      {'label': r'\section{}', 'pre': r'\section{', 'post': '}'},
      {'label': r'\subsection{}', 'pre': r'\subsection{', 'post': '}'},
      {'label': r'\textbf{}', 'pre': r'\textbf{', 'post': '}'},
      {'label': r'\textit{}', 'pre': r'\textit{', 'post': '}'},
      {'label': r'$$ math $$', 'pre': '\$\$\n', 'post': '\n\$\$'},
      {'label': r'\frac{}{}', 'pre': r'\frac{', 'post': '}{}'},
      {'label': r'\align', 'pre': "\\begin{align*}\n", 'post': "\n\\end{align*}"},
      {'label': r'\cite{}', 'pre': r'\cite{', 'post': '}'},
      {'label': r'\ref{}', 'pre': r'\ref{', 'post': '}'},
      {'label': r'\label{}', 'pre': r'\label{', 'post': '}'},
      {'label': r'\item', 'pre': r'\item ', 'post': ''},
      {'label': '% comment', 'pre': '% ', 'post': ''},
    ];

    return Container(
      height: 38,
      decoration: const BoxDecoration(
        color: Color(0xFF21252B),
        border: Border(bottom: BorderSide(color: Colors.white10)),
      ),
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        itemCount: chips.length,
        separatorBuilder: (_, __) => const SizedBox(width: 4),
        itemBuilder: (_, i) {
          final c = chips[i];
          return ActionChip(
            visualDensity: VisualDensity.compact,
            labelPadding: const EdgeInsets.symmetric(horizontal: 4),
            label: Text(
              c['label']!,
              style: const TextStyle(fontSize: 11, fontFamily: 'monospace'),
            ),
            onPressed: () => _insertSnippet(c['pre']!, c['post']!),
          );
        },
      ),
    );
  }

  void _insertSnippet(String prefix, [String suffix = '']) {
    final editor = _editor;
    if (editor == null) return;
    final text = editor.text;
    final sel = editor.selection;
    if (!sel.isValid || sel.start < 0) {
      editor.text = '$text$prefix$suffix';
      editor.selection =
          TextSelection.collapsed(offset: editor.text.length - suffix.length);
      return;
    }
    final selected = sel.textInside(text);
    final replacement = '$prefix$selected$suffix';
    final newText = text.replaceRange(sel.start, sel.end, replacement);
    final newCursor = sel.start + prefix.length + selected.length;
    editor.value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: newCursor),
    );
  }

  Widget _editorPane() {
    final editor = _editor;
    if (editor == null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.code, size: 40, color: Colors.grey),
            const SizedBox(height: 8),
            const Text('Select a file to edit',
                style: TextStyle(color: Colors.grey)),
            if (_entry != null) ...[
              const SizedBox(height: 12),
              FilledButton.tonal(
                onPressed: () => _openProjectFile(_entry!),
                child: Text('Open entry: ${_entry!.split('/').last}'),
              ),
            ],
          ],
        ),
      );
    }
    return Column(
      children: [
        _snippetBar(),
        Expanded(
          child: CodeTheme(
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
          ),
        ),
      ],
    );
  }

  // ---------- pdf pane -------------------------------------------------------

  Widget _pdfPane() {
    final r = _result;
    if (r == null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.picture_as_pdf_outlined,
                size: 48, color: Colors.grey),
            const SizedBox(height: 12),
            const Text('No build yet',
                style: TextStyle(fontSize: 16, color: Colors.grey)),
            const SizedBox(height: 8),
            FilledButton.tonalIcon(
              onPressed: (_entry == null || _busy) ? null : _compile,
              icon: const Icon(Icons.play_arrow),
              label: const Text('Compile Project'),
            ),
          ],
        ),
      );
    }
    if (r['success'] != true) {
      final stderr = r['stderr']?.toString() ?? '';
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.error_outline,
                  size: 48, color: Colors.redAccent),
              const SizedBox(height: 12),
              const Text('Build Failed',
                  style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                      color: Colors.redAccent)),
              const SizedBox(height: 8),
              if (stderr.isNotEmpty)
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Colors.black26,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.red.withValues(alpha: 0.3)),
                  ),
                  child: Text(
                    stderr.length > 300
                        ? '${stderr.substring(0, 300)}...'
                        : stderr,
                    style: const TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 12,
                        color: Colors.white70),
                  ),
                ),
              const SizedBox(height: 12),
              FilledButton.icon(
                onPressed: (_entry == null || _busy) ? null : _compile,
                icon: const Icon(Icons.refresh),
                label: const Text('Retry Compile'),
              ),
            ],
          ),
        ),
      );
    }
    final pdf = r['pdf_path'] as String?;
    if (pdf == null || !File(pdf).existsSync()) {
      return const Center(child: Text('PDF path missing'));
    }
    final modTime = File(pdf).lastModifiedSync().millisecondsSinceEpoch;
    return PdfViewer.file(
      pdf,
      key: ValueKey('$pdf-$modTime'),
    );
  }

  // ---------- diagnostics ---------------------------------------------------

  Widget _diagnosticsPane() {
    final r = _result;
    final corrections = _corrections;
    final diags = (r?['diagnostics'] as List?)?.cast<Map>() ?? const <Map>[];
    final totalIssues = corrections.length + diags.length;

    return DefaultTabController(
      length: 2,
      child: Column(
        children: [
          TabBar(
            isScrollable: true,
            tabAlignment: TabAlignment.start,
            tabs: [
              Tab(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.warning_amber_rounded, size: 16),
                    const SizedBox(width: 4),
                    Text('Problems ($totalIssues)'),
                  ],
                ),
              ),
              const Tab(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.terminal, size: 16),
                    SizedBox(width: 4),
                    Text('Output'),
                  ],
                ),
              ),
            ],
          ),
          Expanded(
            child: TabBarView(
              children: [
                _problemsView(corrections, diags, r),
                _compilerOutputView(r),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _problemsView(
      List<Correction> corrections, List<Map> diags, Map<String, dynamic>? r) {
    final children = <Widget>[];

    if (corrections.isNotEmpty) {
      children.add(Padding(
        padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
        child: Row(
          children: [
            Expanded(
              child: Text(
                'Proposed corrections (${corrections.length})',
                style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 13),
              ),
            ),
            TextButton.icon(
              onPressed: _busy ? null : _applyAllSafeCorrections,
              icon: const Icon(Icons.done_all, size: 16),
              label: const Text('Apply Safe', style: TextStyle(fontSize: 12)),
            ),
          ],
        ),
      ));
      for (final c in corrections) {
        final key = _corrKey(c);
        children.add(CorrectionCard(
          correction: c,
          busy: _applyingKeys.contains(key),
          onApprove: () => _applyCorrection(c),
          onReject: () => _rejectCorrection(c),
          onNavigate: () => _jumpToLocation(c.file, c.line),
        ));
      }
      children.add(const Divider());
    }

    if (r == null) {
      if (children.isEmpty) {
        return const Center(child: Text('No build yet'));
      }
      return ListView(children: children);
    }

    if (r['error'] != null) {
      children.add(Padding(
        padding: const EdgeInsets.all(12),
        child: Text(
          r['error'].toString(),
          style: const TextStyle(color: Colors.red, fontSize: 13),
        ),
      ));
      return ListView(children: children);
    }

    if (diags.isEmpty) {
      children.add(Center(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Text(
            r['success'] == true
                ? 'Build succeeded - no diagnostics'
                : 'Build failed - no parsed diagnostics',
            style: const TextStyle(fontSize: 13),
          ),
        ),
      ));
      return ListView(children: children);
    }

    for (final d in diags) {
      final sev = d['severity']?.toString() ?? '?';
      final loc = d['location'] as Map?;
      final locText = loc == null
          ? ''
          : '${loc['file']}:${loc['line']}'
              '${loc['column'] != null ? ':${loc['column']}' : ''}';
      children.add(ListTile(
        dense: true,
        visualDensity: VisualDensity.compact,
        leading: Icon(
          sev == 'Error' ? Icons.error_outline : Icons.warning_amber,
          color: sev == 'Error' ? Colors.redAccent : Colors.orangeAccent,
          size: 18,
        ),
        title: Text(d['message']?.toString() ?? '',
            style: const TextStyle(fontSize: 13)),
        subtitle: locText.isEmpty
            ? null
            : Text(locText,
                style: const TextStyle(fontSize: 11, fontFamily: 'monospace')),
        trailing: loc != null
            ? const Icon(Icons.arrow_outward, size: 14, color: Colors.grey)
            : null,
        onTap: (loc != null && loc['file'] != null)
            ? () => _jumpToLocation(
                  loc['file']?.toString(),
                  (loc['line'] as num?)?.toInt(),
                  (loc['column'] as num?)?.toInt(),
                )
            : null,
      ));
    }
    return ListView(children: children);
  }

  Widget _compilerOutputView(Map<String, dynamic>? r) {
    if (r == null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.terminal_outlined, size: 36, color: Colors.grey),
            const SizedBox(height: 8),
            Text(
              'No build run yet.\nCompile with ${_engineId.toUpperCase()} to see output.',
              textAlign: TextAlign.center,
              style: const TextStyle(color: Colors.grey, fontSize: 13),
            ),
          ],
        ),
      );
    }

    final stdout = r['stdout']?.toString() ?? '';
    final stderr = r['stderr']?.toString() ?? '';
    final combined = stdout.isEmpty
        ? stderr
        : (stderr.isEmpty ? stdout : '$stdout\n$stderr');

    final success = r['success'] == true;
    final duration = r['duration_ms'] ?? 0;
    final exitCode = r['exit_code'];

    return Column(
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          color: Colors.black26,
          child: Row(
            children: [
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: (success ? Colors.green : Colors.red)
                      .withValues(alpha: 0.2),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  success ? 'SUCCESS' : 'FAILED',
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.bold,
                    color: success ? Colors.green : Colors.redAccent,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Text(
                '${duration}ms',
                style: const TextStyle(fontSize: 11, fontFamily: 'monospace'),
              ),
              if (exitCode != null) ...[
                const SizedBox(width: 8),
                Text(
                  'exit: $exitCode',
                  style: const TextStyle(
                      fontSize: 11, fontFamily: 'monospace', color: Colors.grey),
                ),
              ],
              const Spacer(),
              if (combined.isNotEmpty)
                IconButton(
                  icon: const Icon(Icons.copy, size: 16),
                  tooltip: 'Copy compiler output',
                  visualDensity: VisualDensity.compact,
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: combined));
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(
                        content: Text('Compiler log copied to clipboard'),
                        duration: Duration(seconds: 1),
                      ),
                    );
                  },
                ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            width: double.infinity,
            color: const Color(0xFF1E1E24),
            padding: const EdgeInsets.all(8),
            child: SingleChildScrollView(
              child: SelectableText(
                combined.isEmpty ? '(No compiler output)' : combined,
                style: const TextStyle(
                  fontFamily: 'monospace',
                  fontSize: 12,
                  color: Colors.white70,
                  height: 1.4,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Future<void> _jumpToLocation(String? file, int? line, [int? col]) async {
    if (file == null) return;
    if (_openFile != file) {
      await _openProjectFile(file);
    }
    if (line == null || line <= 0) return;
    final editor = _editor;
    if (editor == null) return;
    final lines = editor.text.split('\n');
    int offset = 0;
    for (int i = 0; i < line - 1 && i < lines.length; i++) {
      offset += lines[i].length + 1;
    }
    if (col != null && col > 0 && line <= lines.length) {
      offset = (offset + col - 1).clamp(0, editor.text.length);
    } else {
      offset = offset.clamp(0, editor.text.length);
    }
    editor.selection = TextSelection.collapsed(offset: offset);
  }

  Future<void> _applyAllSafeCorrections() async {
    final safe = _corrections
        .where((c) => !c.mayChangeMeaning && c.confidence != 'Uncertain')
        .toList();
    if (safe.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
            content: Text('No safe corrections to apply automatically.')),
      );
      return;
    }
    for (final c in safe) {
      await _applyCorrection(c);
    }
  }

  Future<void> _verifyProjectAction() async {
    final root = _projectRoot;
    if (root == null) return;
    setState(() => _busy = true);
    try {
      await _refreshCorrections();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            _corrections.isEmpty
                ? 'Verification clean: no issues detected.'
                : 'Found ${_corrections.length} correction proposal(s).',
          ),
          duration: const Duration(seconds: 2),
        ),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  // ---------- layouts -------------------------------------------------------

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
        SizedBox(width: 360, child: _diagnosticsPane()),
      ],
    );
  }

  Widget _medium([double? maxHeight]) {
    final diagsHeight = (maxHeight != null && maxHeight < 600) ? 140.0 : 180.0;
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
        SizedBox(height: diagsHeight, child: _diagnosticsPane()),
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

  // ---------- app bar / build ------------------------------------------------

  String _windowTitle({bool short = false}) {
    final dirtyMark = _dirty ? '• ' : '';
    if (short) {
      if (_openFile != null) {
        final f = _openFile!.split('/').last;
        return '$dirtyMark$f';
      }
      if (_projectRoot != null) {
        final sep = Platform.pathSeparator;
        final parts =
            _projectRoot!.split(sep).where((s) => s.isNotEmpty).toList();
        return parts.isEmpty ? 'LaTeX Studio' : parts.last;
      }
      return 'LaTeX Studio';
    }
    final buf = StringBuffer('LaTeX Studio');
    if (_projectRoot != null) {
      final sep = Platform.pathSeparator;
      final parts =
          _projectRoot!.split(sep).where((s) => s.isNotEmpty).toList();
      final name = parts.isEmpty ? _projectRoot : parts.last;
      buf.write(' \u2014 $name');
      if (_openFile != null) {
        buf.write(' / ${_openFile!.split('/').last}');
      }
      if (_dirty) buf.write(' \u2022');
    }
    return buf.toString();
  }

  List<Widget> _wideActions() {
    return [
      TextButton.icon(
        onPressed: _busy ? null : _changeEngineDialog,
        icon: const Icon(Icons.tune, size: 16),
        label: Text(
          _engineId.toUpperCase(),
          style: const TextStyle(fontSize: 11, fontWeight: FontWeight.bold),
        ),
      ),
      if (_busy)
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 8),
          child: Center(
            child: SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
          ),
        ),
      IconButton(
        visualDensity: VisualDensity.compact,
        onPressed: _busy ? null : _newFromTemplate,
        icon: const Icon(Icons.note_add_outlined),
        tooltip: 'New from template',
      ),
      IconButton(
        visualDensity: VisualDensity.compact,
        onPressed: _busy ? null : _pickProject,
        icon: const Icon(Icons.folder_open),
        tooltip: 'Open project (Ctrl+O)',
      ),
      IconButton(
        visualDensity: VisualDensity.compact,
        onPressed: (_editor == null || _busy) ? null : _save,
        icon: Icon(_dirty ? Icons.save : Icons.save_outlined),
        tooltip: 'Save (Ctrl+S)',
      ),
      IconButton(
        visualDensity: VisualDensity.compact,
        onPressed:
            (_projectRoot == null || _busy) ? null : _verifyProjectAction,
        icon: const Icon(Icons.auto_fix_high_outlined),
        tooltip: 'Verify project & propose corrections',
      ),
      FilledButton.icon(
        style: FilledButton.styleFrom(visualDensity: VisualDensity.compact),
        onPressed: (_entry == null || _busy) ? null : _compile,
        icon: const Icon(Icons.play_arrow, size: 16),
        label: const Text('Compile'),
      ),
      const SizedBox(width: 2),
      IconButton(
        visualDensity: VisualDensity.compact,
        onPressed: (_projectRoot == null || _busy) ? null : _publish,
        icon: const Icon(Icons.archive_outlined),
        tooltip: 'Publish archive',
      ),
      const SizedBox(width: 4),
    ];
  }

  List<Widget> _compactActions() {
    return [
      if (_busy)
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 10),
          child: Center(
            child: SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
          ),
        ),
      if (_editor != null)
        IconButton(
          onPressed: _busy ? null : _save,
          icon: Icon(_dirty ? Icons.save : Icons.save_outlined),
          tooltip: 'Save',
        ),
      IconButton(
        onPressed: (_entry == null || _busy) ? null : _compile,
        icon: const Icon(Icons.play_arrow),
        tooltip: 'Compile',
      ),
      PopupMenuButton<String>(
        icon: const Icon(Icons.more_vert),
        onSelected: (val) {
          switch (val) {
            case 'open':
              _pickProject();
              break;
            case 'path':
              _showPathDialog();
              break;
            case 'engine':
              _changeEngineDialog();
              break;
            case 'template':
              _newFromTemplate();
              break;
            case 'verify':
              _verifyProjectAction();
              break;
            case 'publish':
              _publish();
              break;
          }
        },
        itemBuilder: (ctx) => [
          const PopupMenuItem(value: 'open', child: Text('Open project...')),
          const PopupMenuItem(value: 'path', child: Text('Open by path...')),
          const PopupMenuItem(value: 'engine', child: Text('Select engine...')),
          const PopupMenuItem(
              value: 'template', child: Text('New from template...')),
          const PopupMenuDivider(),
          const PopupMenuItem(
              value: 'verify', child: Text('Verify & propose fixes')),
          const PopupMenuItem(
              value: 'publish', child: Text('Publish archive...')),
        ],
      ),
    ];
  }

  Widget _pathField() {
    return Padding(
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
    );
  }

  Future<void> _showPathDialog() async {
    final controller = TextEditingController(text: _projectRoot ?? '');
    final path = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Enter project directory'),
        content: TextField(
          controller: controller,
          decoration: const InputDecoration(
            labelText: 'Path',
            border: OutlineInputBorder(),
          ),
          autofocus: true,
          onSubmitted: (v) => Navigator.of(ctx).pop(v),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(ctx).pop(controller.text),
            child: const Text('Load'),
          ),
        ],
      ),
    );
    if (path != null && path.trim().isNotEmpty) {
      _pathController.text = path.trim();
      await _loadByPath();
    }
  }

  Future<void> _changeEngineDialog() async {
    final chosen = await showDialog<String>(
      context: context,
      builder: (ctx) => SimpleDialog(
        title: const Text('Select Compiler Engine'),
        children: _engines.map((e) {
          final id = e['id'] as String;
          final name = e['display_name'] as String;
          final avail = e['available'] == true;
          return SimpleDialogOption(
            onPressed: avail ? () => Navigator.of(ctx).pop(id) : null,
            child: Row(
              children: [
                Icon(
                  id == _engineId
                      ? Icons.radio_button_checked
                      : Icons.radio_button_off,
                  size: 18,
                  color: id == _engineId
                      ? Theme.of(context).colorScheme.primary
                      : null,
                ),
                const SizedBox(width: 8),
                Text(avail ? name : '$name (unavailable)'),
              ],
            ),
          );
        }).toList(),
      ),
    );
    if (chosen != null && mounted) {
      setState(() => _engineId = chosen);
    }
  }

  Widget _bottomStatusBar() {
    final editor = _editor;
    final r = _result;
    final lineCount = editor == null ? 0 : editor.text.split('\n').length;
    final charCount = editor == null ? 0 : editor.text.length;

    Color statusColor = Colors.green;
    String statusText = 'Ready';
    if (_busy) {
      statusColor = Colors.blue;
      statusText = 'Working...';
    } else if (_dirty) {
      statusColor = Colors.amber;
      statusText = 'Unsaved changes';
    } else if (r != null && r['success'] != true) {
      statusColor = Colors.redAccent;
      statusText = 'Build failed';
    }

    final diags = (r?['diagnostics'] as List?)?.length ?? 0;
    final issuesCount = _corrections.length + diags;

    return Container(
      height: 28,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      decoration: const BoxDecoration(
        color: Color(0xFF1E2028),
        border: Border(top: BorderSide(color: Colors.white10)),
      ),
      child: Row(
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              color: statusColor,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            statusText,
            style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w500),
          ),
          if (_openFile != null) ...[
            const SizedBox(width: 12),
            Text(
              _openFile!,
              style: const TextStyle(fontSize: 11, color: Colors.white70),
            ),
          ],
          const Spacer(),
          if (issuesCount > 0) ...[
            Icon(
              Icons.warning_amber_rounded,
              size: 14,
              color: _corrections.isNotEmpty
                  ? Colors.lightBlueAccent
                  : Colors.orangeAccent,
            ),
            const SizedBox(width: 4),
            Text(
              '$issuesCount issue${issuesCount == 1 ? '' : 's'}',
              style: TextStyle(
                fontSize: 11,
                color: _corrections.isNotEmpty
                  ? Colors.lightBlueAccent
                  : Colors.orangeAccent,
              ),
            ),
            const SizedBox(width: 16),
          ],
          if (editor != null) ...[
            Text(
              'Lines: $lineCount  Chars: $charCount',
              style: const TextStyle(
                  fontSize: 11, color: Colors.white54, fontFamily: 'monospace'),
            ),
            const SizedBox(width: 16),
          ],
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(
              color: Colors.white10,
              borderRadius: BorderRadius.circular(3),
            ),
            child: Text(
              _engineId.toUpperCase(),
              style: const TextStyle(
                  fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 0.5),
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final r = _result;
    final size = MediaQuery.of(context).size;
    final isCompact = size.width < 960 || size.height < 500;

    return Shortcuts(
      shortcuts: const <ShortcutActivator, Intent>{
        SingleActivator(LogicalKeyboardKey.keyS, control: true):
            _SaveIntent(),
        SingleActivator(LogicalKeyboardKey.enter, control: true):
            _CompileIntent(),
        SingleActivator(LogicalKeyboardKey.keyO, control: true):
            _OpenProjectIntent(),
      },
      child: Actions(
        actions: <Type, Action<Intent>>{
          _SaveIntent: CallbackAction<_SaveIntent>(
            onInvoke: (_) {
              if (_editor != null && !_busy) _save();
              return null;
            },
          ),
          _CompileIntent: CallbackAction<_CompileIntent>(
            onInvoke: (_) {
              if (_entry != null && !_busy) _compile();
              return null;
            },
          ),
          _OpenProjectIntent: CallbackAction<_OpenProjectIntent>(
            onInvoke: (_) {
              if (!_busy) _pickProject();
              return null;
            },
          ),
        },
        child: Focus(
          autofocus: true,
          child: Scaffold(
            appBar: AppBar(
              title: Text(
                _windowTitle(short: isCompact),
                overflow: TextOverflow.ellipsis,
              ),
              actions: isCompact ? _compactActions() : _wideActions(),
            ),
            body: Column(
              children: [
                if (_nativeError != null)
                  MaterialBanner(
                    content: Text(_nativeError!),
                    actions: const [SizedBox.shrink()],
                  ),
                if (!isCompact) _pathField(),
                if (r != null && r['error'] != null)
                  MaterialBanner(
                    content: Text(r['error'].toString()),
                    actions: const [SizedBox.shrink()],
                  ),
                Expanded(
                  child: LayoutBuilder(
                    builder: (_, c) {
                      if (c.maxWidth >= 1100 && c.maxHeight >= 500) {
                        return _wide();
                      }
                      if (c.maxWidth >= 700 && c.maxHeight >= 450) {
                        return _medium(c.maxHeight);
                      }
                      return _narrow();
                    },
                  ),
                ),
                _bottomStatusBar(),
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

class _CompileIntent extends Intent {
  const _CompileIntent();
}

class _OpenProjectIntent extends Intent {
  const _OpenProjectIntent();
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
