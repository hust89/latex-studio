import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:latex_studio/correction_card.dart';
import 'package:latex_studio/models.dart';
import 'package:latex_studio/main.dart';

void main() {
  group('Models Serialization & Logic', () {
    test('Diagnostic parsing and round-trip', () {
      final json = {
        'severity': 'Error',
        'kind': 'UnresolvedReference',
        'message': 'Unresolved reference: fig:arch',
        'explanation': 'Did you mean fig:architecture?',
        'location': {
          'file': 'main.tex',
          'line': 42,
          'column': 10,
        },
      };

      final diag = Diagnostic.fromJson(json);
      expect(diag.isError, isTrue);
      expect(diag.isWarning, isFalse);
      expect(diag.message, 'Unresolved reference: fig:arch');
      expect(diag.file, 'main.tex');
      expect(diag.line, 42);
      expect(diag.column, 10);

      final outJson = diag.toJson();
      expect(outJson['severity'], 'Error');
      expect(outJson['message'], 'Unresolved reference: fig:arch');
      expect(outJson['location']['file'], 'main.tex');
    });

    test('Correction previewAfter and deletion detection', () {
      const c = Correction(
        diagnostic: Diagnostic(
          severity: 'Warning',
          kind: 'UnusedLabel',
          message: 'Unused label',
        ),
        explanation: 'Remove unused label',
        file: 'intro.tex',
        line: 12,
        context: r'Here is \label{sec:old} text',
        oldText: r'\label{sec:old}',
        newText: '',
        confidence: 'Certain',
        mayChangeMeaning: false,
        alternatives: [],
      );

      expect(c.isDeletion, isTrue);
      expect(c.previewAfter(), 'Here is  text');
    });

    test('Alternative parsing', () {
      final alt = Alternative.fromJson({
        'value': r'\alpha',
        'explanation': 'Greek letter alpha',
        'confidence': 'Likely',
      });
      expect(alt.value, r'\alpha');
      expect(alt.confidence, 'Likely');
    });
  });

  group('CorrectionCard Widget', () {
    testWidgets('renders correction details and triggers callbacks',
        (WidgetTester tester) async {
      bool approved = false;
      bool rejected = false;
      bool navigated = false;

      const corr = Correction(
        diagnostic: Diagnostic(
          severity: 'Error',
          kind: 'Syntax',
          message: 'Missing dollar',
        ),
        explanation: 'Insert math delimiter',
        file: 'paper.tex',
        line: 88,
        context: 'Let x = 5 be an integer',
        oldText: 'x = 5',
        newText: r'$x = 5$',
        confidence: 'Certain',
        mayChangeMeaning: false,
        alternatives: [],
      );

      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: CorrectionCard(
            correction: corr,
            onApprove: () => approved = true,
            onReject: () => rejected = true,
            onNavigate: () => navigated = true,
          ),
        ),
      ));

      expect(find.text('paper.tex:88'), findsOneWidget);
      expect(find.text('Insert math delimiter'), findsOneWidget);
      expect(find.text('Certain'), findsOneWidget);

      await tester.tap(find.text('paper.tex:88'));
      expect(navigated, isTrue);

      await tester.tap(find.text('Apply'));
      expect(approved, isTrue);

      await tester.tap(find.text('Reject'));
      expect(rejected, isTrue);
    });
  });

  group('LatexApp Smoke & Responsive Layout', () {
    testWidgets('smoke test renders app on wide desktop canvas',
        (WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);

      await tester.pumpWidget(const LatexApp());
      expect(find.textContaining('LaTeX Studio'), findsWidgets);
      expect(find.text('PROJECT FILES'), findsNothing); // before load
      expect(find.text('No project loaded'), findsWidgets);
    });

    testWidgets('smoke test renders app on compact mobile canvas',
        (WidgetTester tester) async {
      tester.view.physicalSize = const Size(400, 700);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);

      await tester.pumpWidget(const LatexApp());
      expect(find.textContaining('LaTeX Studio'), findsWidgets);
    });
  });
}
