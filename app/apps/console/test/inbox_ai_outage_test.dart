import 'package:console/screens/inbox.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Future<void> _pumpInbox(
  WidgetTester tester, {
  required int errorsLastHour,
}) async {
  setViewport(tester, wideSize);
  final fake = FakeApi()
    ..reply('GET', '/admin/threads', [threadJson(id: 'th-1', topic: 'studio')])
    ..reply('GET', '/admin/ai/usage', {
      'configured': true,
      'mode': 'always',
      'month': {
        'calls': 7,
        'errors': errorsLastHour,
        'errors_last_hour': errorsLastHour,
      },
    });
  stockConsoleApi(fake);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(tester, env, const InboxScreen());
  await pumpUntilFound(tester, find.text(koT.aiUsageMonth('7')));
}

void main() {
  testWidgets(
    'the inbox shows the outage banner while AI calls failed in the last hour',
    (tester) async {
      await _pumpInbox(tester, errorsLastHour: 2);

      expect(find.text(koT.aiOutageBanner), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('no failed AI calls in the last hour means no outage banner', (
    tester,
  ) async {
    await _pumpInbox(tester, errorsLastHour: 0);

    expect(find.text(koT.aiOutageBanner), findsNothing);
    await unmount(tester);
  });
}
