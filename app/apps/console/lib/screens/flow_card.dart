// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

/// Booking flow figures from the API's `flow` object (dashboard and status page). Null when the API could not read them.
class FlowCard extends StatelessWidget {
  const FlowCard({super.key, required this.flow});
  final Json? flow;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final f = flow;
    if (f == null) {
      return AppCard(
        child: KeepAllText(t.oFlowUnavailable, style: context.text.bodyMedium),
      );
    }
    final median = f['median_hours_to_assign'] as num?;
    final sameDay = (f['same_day'] as Json?) ?? const {};
    final services = (f['no_show_by_service'] as List? ?? const [])
        .cast<Json>();
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(t.oFlowTitle, style: context.text.titleMedium),
          const SizedBox(height: Space.md),
          _FlowRow(
            label: t.oFlowAssign,
            value: median == null
                ? t.oFlowNoAssignYet
                : t.oFlowAssignValue(median.toStringAsFixed(1)),
          ),
          _FlowRow(
            label: t.oFlowUnassigned,
            value: t.oFlowUnassignedValue('${_int(f['unassigned_over_24h'])}'),
          ),
          _FlowRow(
            label: t.oFlowSameDay,
            value: t.oFlowSameDayValue(
              '${_int(sameDay['cancelled'])}',
              '${_int(sameDay['booked'])}',
            ),
          ),
          const SizedBox(height: Space.xs),
          KeepAllText(t.oFlowNoShow, style: context.text.labelLarge),
          const SizedBox(height: Space.xxs),
          if (services.isEmpty)
            KeepAllText(t.oFlowNoBookings, style: context.text.bodySmall),
          for (final s in services)
            _FlowRow(
              label: t.service(s['service_code'] as String),
              value: t.oFlowNoShowValue(
                '${_int(s['no_shows'])}',
                '${_int(s['bookings'])}',
                (_int(s['rate_bps']) / 100).toStringAsFixed(1),
              ),
            ),
        ],
      ),
    );
  }
}

int _int(Object? v) => (v as num? ?? 0).toInt();

class _FlowRow extends StatelessWidget {
  const _FlowRow({required this.label, required this.value});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: Space.xxs),
    child: Row(
      children: [
        Expanded(child: KeepAllText(label, style: context.text.bodyMedium)),
        const SizedBox(width: Space.xs),
        KeepAllText(value, style: context.text.titleSmall),
      ],
    ),
  );
}
