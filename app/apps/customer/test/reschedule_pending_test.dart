import 'package:core/core.dart';
import 'package:customer/screens/booking_detail.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

final _day = DateUtils.dateOnly(DateTime.now()).add(const Duration(days: 5));
final _to = _day.add(const Duration(days: 2));

/// The request a pro-assigned reschedule leaves: the visit keeps its slot until the pro confirms.
Json _pending() => {
  'from_date': isoDay(_day),
  'from_hour': 9,
  'to_date': isoDay(_to),
  'to_hour': 14,
  'requested_at': '2026-11-17T01:02:03.456789Z',
};

Future<FakeApi> _pumpDetail(
  WidgetTester tester,
  Json booking, {
  Json? afterReschedule,
}) async {
  final fake = FakeApi()
    ..reply('GET', '/bookings/${booking['id']}', booking)
    ..reply('GET', '/bookings/${booking['id']}/cancel-preview', {
      'can_cancel': true,
      'days_before': 5,
      'percent': null,
      'paid': 36300,
      'fee': 0,
      'refund': 36300,
      'reschedule_allowed': true,
    })
    ..reply('GET', '/slots', [slotJson(9)]);
  if (afterReschedule != null) {
    fake.reply(
      'POST',
      '/bookings/${booking['id']}/reschedule',
      afterReschedule,
    );
  }
  stockCustomerApi(fake, bookings: [booking]);
  final env = await TestEnv.create(fake: fake);
  final app = await pumpCustomerScreen(
    tester,
    env,
    BookingDetailScreen(id: booking['id'] as String),
  );
  await app.loadServices();
  await pumpUntilFound(tester, find.byType(Pulse));
  return fake;
}

Future<void> _rescheduleToSameSlot(WidgetTester tester) async {
  final t = lookupT(const Locale('ko'));
  final entry = find.text(t.reschedule);
  // The detail list builds lazily: scroll down to the 일정 변경 entry.
  await settle(tester);
  await tester.scrollUntilVisible(
    entry,
    300,
    scrollable: find.byType(Scrollable).first,
  );
  await settle(tester);
  await tester.tap(entry);
  await settle(tester);
  await tester.tap(find.text(t.next));
  await settle(tester);
}

void main() {
  final t = lookupT(const Locale('ko'));

  testWidgets(
    'booking detail shows a pending reschedule as waiting for the pro, with the requested slot',
    (tester) async {
      setViewport(tester, phoneSize);
      await _pumpDetail(
        tester,
        bookingJson(
          id: 'b1',
          status: 2,
          visitDate: _day,
          reschedule: _pending(),
        ),
      );

      expect(find.text(t.reschedulePending), findsOneWidget);
      expect(
        find.text(
          t.reschedulePendingTo(
            '${dateLabel(_to, 'ko')} ${slotRangeLabel(14, 'ko')}',
          ),
        ),
        findsOneWidget,
      );
      await unmount(tester);
    },
  );

  testWidgets('booking detail without a pending request shows no such card', (
    tester,
  ) async {
    setViewport(tester, phoneSize);
    await _pumpDetail(
      tester,
      bookingJson(id: 'b1', status: 2, visitDate: _day),
    );

    expect(find.text(t.reschedulePending), findsNothing);
    await unmount(tester);
  });

  testWidgets(
    'a reschedule the pro still has to confirm says the request was sent, not that the visit moved',
    (tester) async {
      setViewport(tester, phoneSize);
      final fake = await _pumpDetail(
        tester,
        bookingJson(id: 'b1', status: 2, visitDate: _day),
        afterReschedule: bookingJson(
          id: 'b1',
          status: 2,
          visitDate: _day,
          reschedule: _pending(),
        ),
      );
      await _rescheduleToSameSlot(tester);

      expect(fake.callsTo('POST', '/bookings/b1/reschedule'), hasLength(1));
      expect(find.text(t.rescheduleRequested), findsOneWidget);
      expect(find.text(t.rescheduled), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets('a reschedule with no pro assigned yet says the visit moved', (
    tester,
  ) async {
    setViewport(tester, phoneSize);
    final fake = await _pumpDetail(
      tester,
      bookingJson(id: 'b1', status: 1, visitDate: _day),
      afterReschedule: bookingJson(id: 'b1', status: 1, visitDate: _day),
    );
    await _rescheduleToSameSlot(tester);

    expect(fake.callsTo('POST', '/bookings/b1/reschedule'), hasLength(1));
    expect(find.text(t.rescheduled), findsOneWidget);
    await unmount(tester);
  });
}
