import 'package:flutter_test/flutter_test.dart';
import 'package:latex_studio/main.dart';

void main() {
  testWidgets('LaTeX Studio smoke test', (WidgetTester tester) async {
    await tester.pumpWidget(const LatexApp());
    expect(find.textContaining('LaTeX Studio'), findsWidgets);
  });
}
