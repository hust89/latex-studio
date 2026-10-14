/// Dart-side mirrors of the JSON shapes produced by liblatex_ffi.
///
/// These are not the source of truth — the Rust types are — but parsing
/// into typed objects makes the UI code much simpler than poking at
/// `Map<String, dynamic>` everywhere.

// ---------- project ----------------------------------------------------------

class ProjectFile {
  final String path;
  final String kind;
  const ProjectFile(this.path, this.kind);
}

class ProjectIndex {
  final List<ProjectFile> files;
  final String? entry;
  const ProjectIndex(this.files, this.entry);
}

class Recoverable {
  final String path;
  final int autosaveMtimeMs;
  final int originalMtimeMs;
  const Recoverable(this.path, this.autosaveMtimeMs, this.originalMtimeMs);
}

// ---------- diagnostics ------------------------------------------------------

class Diagnostic {
  final String severity; // Error | Warning | Info
  final String kind;
  final String message;
  final String? explanation;
  final String? file;
  final int? line;
  final int? column;

  const Diagnostic({
    required this.severity,
    required this.kind,
    required this.message,
    this.explanation,
    this.file,
    this.line,
    this.column,
  });

  factory Diagnostic.fromJson(Map<String, dynamic> j) {
    final loc = j['location'] as Map?;
    return Diagnostic(
      severity: j['severity'] as String? ?? 'Info',
      kind: j['kind'] as String? ?? 'Other',
      message: j['message'] as String? ?? '',
      explanation: j['explanation'] as String?,
      file: loc?['file'] as String?,
      line: (loc?['line'] as num?)?.toInt(),
      column: (loc?['column'] as num?)?.toInt(),
    );
  }

  Map<String, dynamic> toJson() => {
        'severity': severity,
        'kind': kind,
        'message': message,
        'explanation': explanation,
        'location': file == null
            ? null
            : {
                'file': file,
                'line': line ?? 1,
                'column': column,
              },
        'raw': null,
      };

  bool get isError => severity == 'Error';
  bool get isWarning => severity == 'Warning';
  bool get isInfo => severity == 'Info';
}

// ---------- corrections ------------------------------------------------------

class Alternative {
  final String value;
  final String explanation;
  final String confidence;
  const Alternative({
    required this.value,
    required this.explanation,
    required this.confidence,
  });

  factory Alternative.fromJson(Map<String, dynamic> j) => Alternative(
        value: j['value'] as String? ?? '',
        explanation: j['explanation'] as String? ?? '',
        confidence: j['confidence'] as String? ?? 'Likely',
      );
}

class Correction {
  final Diagnostic diagnostic;
  final String explanation;
  final String file;
  final int line;
  final String context;
  final String oldText;
  final String newText;
  final String confidence;
  final bool mayChangeMeaning;
  final List<Alternative> alternatives;

  const Correction({
    required this.diagnostic,
    required this.explanation,
    required this.file,
    required this.line,
    required this.context,
    required this.oldText,
    required this.newText,
    required this.confidence,
    required this.mayChangeMeaning,
    required this.alternatives,
  });

  factory Correction.fromJson(Map<String, dynamic> j) => Correction(
        diagnostic:
            Diagnostic.fromJson((j['diagnostic'] as Map).cast<String, dynamic>()),
        explanation: j['explanation'] as String? ?? '',
        file: j['file'] as String? ?? '',
        line: (j['line'] as num?)?.toInt() ?? 0,
        context: j['context'] as String? ?? '',
        oldText: j['old_text'] as String? ?? '',
        newText: j['new_text'] as String? ?? '',
        confidence: j['confidence'] as String? ?? 'Likely',
        mayChangeMeaning: j['may_change_meaning'] as bool? ?? false,
        alternatives: ((j['alternatives'] as List?) ?? const [])
            .map((a) => Alternative.fromJson((a as Map).cast<String, dynamic>()))
            .toList(),
      );

  /// Raw JSON in the exact shape `latex_correct_apply` expects.
  Map<String, dynamic> toJson() => {
        'diagnostic': diagnostic.toJson(),
        'explanation': explanation,
        'file': file,
        'line': line,
        'context': context,
        'old_text': oldText,
        'new_text': newText,
        'confidence': confidence,
        'may_change_meaning': mayChangeMeaning,
        'alternatives': alternatives
            .map((a) => {
                  'value': a.value,
                  'explanation': a.explanation,
                  'confidence': a.confidence,
                })
            .toList(),
      };

  /// The line with the change applied, for preview purposes.
  String previewAfter() => context.replaceFirst(oldText, newText);

  bool get isDeletion => newText.isEmpty;
}
