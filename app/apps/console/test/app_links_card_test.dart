import 'package:console/screens/setup_app_links.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

void main() {
  testWidgets(
    'the app links card lays out its save button under the console theme',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi()
        ..reply('GET', '/admin/app-links', {
          'android_cert_sha256': <String>[],
          'apple_team_id': null,
          'source': 'env',
        });
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(
        tester,
        env,
        const SingleChildScrollView(child: AppLinksCard()),
      );
      await settle(tester);

      expect(
        tester.takeException(),
        isNull,
        reason: 'the full-width button theme must not break the row',
      );
      final save = find.widgetWithText(FilledButton, koT.save);
      expect(save, findsOneWidget);
      expect(tester.getSize(save).width, greaterThan(0));
      await unmount(tester);
    },
  );
}
