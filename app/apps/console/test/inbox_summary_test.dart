import 'package:console/screens/inbox.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Json _quoteMessage() => chatMessageJson(
  id: 'm-quote',
  kind: 'quote',
  body: {'booking_id': 'bk-1', 'final_price': 363000, 'deposit': 36300},
);

void main() {
  testWidgets(
    'every thread row shows its last-message time, even with unread messages (K-03)',
    (tester) async {
      setViewport(tester, wideSize);
      final thread = threadJson(id: 'th-1', topic: 'studio', unreadStaff: 2);
      final fake = FakeApi()
        ..reply('GET', '/admin/threads', [thread])
        ..reply('GET', '/admin/threads/th-1/messages', const <Json>[])
        ..reply('POST', '/admin/threads/th-1/read', null);
      stockConsoleApi(fake);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(tester, env, const InboxScreen());
      await pumpUntilFound(tester, find.text(koT.service('studio')));

      final row = find.byType(ListTile).first;
      expect(
        find.descendant(of: row, matching: find.byType(Badge)),
        findsOneWidget,
      );
      final time = timeLabel(
        DateTime.parse(thread['last_message_at'] as String).toLocal(),
        'ko',
      );
      expect(
        find.descendant(of: row, matching: find.text(time)),
        findsOneWidget,
        reason: 'time stays on an unread row',
      );
      await unmount(tester);
    },
  );

  testWidgets(
    'conversation shows a booking summary with customer, service, address, amount and the sent-quote chip (K-03, K-04)',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi()
        ..reply('GET', '/admin/threads', [
          threadJson(id: 'th-1', topic: 'move_out', bookingId: 'bk-1'),
        ])
        ..reply('GET', '/admin/threads/th-1/messages', [_quoteMessage()])
        ..reply('POST', '/admin/threads/th-1/read', null)
        ..onGet(
          '/admin/bookings/bk-1',
          (_) => {
            ...bookingDetail(status: 0),
            'booking': bookingJson(
              id: 'bk-1',
              serviceCode: 'move_out',
              status: 0,
              estimate: 330000,
              finalPrice: 363000,
            ),
          },
        );
      stockConsoleApi(fake);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(tester, env, const InboxScreen());
      await pumpUntilFound(tester, find.text(koT.service('move_out')));
      await tester.sendKeyEvent(LogicalKeyboardKey.keyJ);
      await settle(tester);

      expect(find.text(koT.inboxSummaryTitle), findsOneWidget);
      expect(
        find.text(koT.quoteSentChip),
        findsOneWidget,
        reason: 'status 0 with a sent quote',
      );
      expect(find.text('지민 · 010-1234-5678'), findsOneWidget);
      expect(find.text(koT.inboxSummaryAmount), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets(
    'a sent quote is a timeline entry with resend and revise actions for staff (K-04)',
    (tester) async {
      setViewport(tester, wideSize);
      final fake = FakeApi()
        ..reply('GET', '/admin/threads', [
          threadJson(id: 'th-1', topic: 'move_out', bookingId: 'bk-1'),
        ])
        ..reply('GET', '/admin/threads/th-1/messages', [_quoteMessage()])
        ..reply('POST', '/admin/threads/th-1/read', null)
        ..onGet('/admin/bookings/bk-1', (_) => bookingDetail(status: 0));
      stockConsoleApi(fake);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(tester, env, const InboxScreen());
      await pumpUntilFound(tester, find.text(koT.service('move_out')));
      await tester.sendKeyEvent(LogicalKeyboardKey.keyJ);
      await settle(tester);

      expect(
        find.textContaining(koT.quoteSentEntry('X', 'Y').split('X').first),
        findsOneWidget,
      );
      expect(find.widgetWithText(TextButton, koT.quoteResend), findsOneWidget);
      expect(find.widgetWithText(TextButton, koT.quoteRevise), findsOneWidget);
      await unmount(tester);
    },
  );
}
