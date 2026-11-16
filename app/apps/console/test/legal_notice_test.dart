import 'package:console/shell.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

void main() {
  testWidgets(
    'the sidebar shows the owner copyright line and its licenses row opens the page',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi();
      stockConsoleApi(fake);
      final env = await TestEnv.create(fake: fake);
      await pumpConsoleApp(tester, env);
      await settle(tester);
      expect(find.byType(ConsoleShell), findsOneWidget);
      expect(find.text(koT.legalCopyright), findsOneWidget);

      await tester.tap(find.text(koT.moreLicenses));
      await settle(tester);
      expect(find.text(koT.legalCopyright), findsWidgets);
      await unmount(tester);
    },
  );
}
