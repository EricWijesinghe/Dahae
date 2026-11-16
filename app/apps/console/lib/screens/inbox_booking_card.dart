import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../booking_labels.dart';
import '../state.dart';

/// Booking summary above a conversation: customer, visit, service, address, amount and status (K-03).
/// Collapsible; open by default on wide layouts. Reloads when the conversation gets a new message.
class InboxBookingCard extends StatelessWidget {
  const InboxBookingCard({super.key, required this.bookingId});

  final String bookingId;

  @override
  Widget build(BuildContext context) => AsyncView<Json>(
    load: () async =>
        await context.api.get('/admin/bookings/$bookingId') as Json,
    builder: (context, data, _) => _Summary(data: data),
  );
}

class _Summary extends StatelessWidget {
  const _Summary({required this.data});

  final Json data;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final b = Booking.fromJson(data['booking'] as Json);
    final customer = data['customer'] as Json;
    final phone = (customer['phone'] as String?)?.trim() ?? '';
    final amount = b.finalPrice ?? b.estimate;
    final visit =
        '${dateLabel(b.visitDate, context.lang)} ${hourLabel(b.startHour, context.lang)}';
    final quoteSent = b.status == 0 && b.finalPrice != null;
    final rows = <(String, String)>[
      (
        t.inboxSummaryCustomer,
        [
          customerNameLabel(t, customer['name'] as String?),
          if (phone.isNotEmpty) formatKoreanPhone(phone),
        ].join(' · '),
      ),
      (t.inboxSummaryService, t.service(b.serviceCode)),
      (t.inboxSummaryVisit, visit),
      (
        t.inboxSummaryAddress,
        '${b.address['road_address'] ?? ''} ${b.address['detail'] ?? ''}'
            .trim(),
      ),
      (t.inboxSummaryAmount, amount == null ? '-' : context.won(amount)),
    ];
    return ExpansionTile(
      initiallyExpanded: context.isWide,
      tilePadding: const EdgeInsets.symmetric(horizontal: Space.lg),
      childrenPadding: const EdgeInsets.fromLTRB(
        Space.lg,
        0,
        Space.lg,
        Space.md,
      ),
      expandedCrossAxisAlignment: CrossAxisAlignment.start,
      title: Text(t.inboxSummaryTitle, style: context.text.titleSmall),
      subtitle: Text(
        '${t.service(b.serviceCode)} · $visit',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      children: [
        for (final (label, value) in rows) _SummaryRow(label, value),
        _SummaryRow(
          t.inboxSummaryStatus,
          t.status(b.status),
          chip: quoteSent ? t.quoteSentChip : null,
        ),
      ],
    );
  }
}

class _SummaryRow extends StatelessWidget {
  const _SummaryRow(this.label, this.value, {this.chip});

  final String label;
  final String value;
  final String? chip;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: Space.xs),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(width: 72, child: Text(label, style: context.text.labelSmall)),
        Expanded(child: Text(value, style: context.text.bodyMedium)),
        if (chip != null)
          Chip(label: Text(chip!), visualDensity: VisualDensity.compact),
      ],
    ),
  );
}
