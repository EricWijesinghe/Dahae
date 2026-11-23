// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/owner_totp_dialog.dart';

final _hhmm = RegExp(r'^([01]\d|2[0-3]):[0-5]\d$');

/// The Toss payouts switch (settings row `payout_toss_switch`). Each flip needs the owner's code; the
/// server also accepts the old env switch, so this can only add, never remove, an env-enabled setup.
class TossPayoutSwitchCard extends StatefulWidget {
  const TossPayoutSwitchCard({super.key, required this.on});
  final bool on;

  @override
  State<TossPayoutSwitchCard> createState() => _TossPayoutSwitchCardState();
}

class _TossPayoutSwitchCardState extends State<TossPayoutSwitchCard> {
  late bool _on = widget.on;

  Future<void> _flip(bool v) async {
    final api = context.api, t = context.t;
    final ok = await runOwnerAction(context, (h) async {
      await api.put('/admin/setup/payout/switch', {'on': v}, h);
      return true;
    }, done: v ? t.cRbPayoutOn : t.cRbPayoutOff);
    if (ok == true && mounted) setState(() => _on = v);
  }

  @override
  Widget build(BuildContext context) => AppCard(
    child: SwitchListTile(
      contentPadding: EdgeInsets.zero,
      title: KeepAllText(context.t.cRbPayoutSwitch, style: context.text.titleSmall),
      subtitle: KeepAllText(context.t.cRbPayoutSwitchHint),
      value: _on,
      onChanged: _flip,
    ),
  );
}

/// The checks the server makes on business hours, so the owner sees a mistake before saving.
bool validBusinessHours(Set<int> weekdays, String open, String close) =>
    weekdays.isNotEmpty &&
    _hhmm.hasMatch(open) &&
    _hhmm.hasMatch(close) &&
    close.compareTo(open) > 0;

/// Business hours (`/admin/setup/business-hours`), read by the AI chat's closed-hours mode.
/// Weekdays are 0 = Monday ... 6 = Sunday, as the server reads them.
class BusinessHoursCard extends StatelessWidget {
  const BusinessHoursCard({super.key});

  @override
  Widget build(BuildContext context) => AsyncView<Json>(
    load: () async =>
        await context.api.get('/admin/setup/business-hours') as Json,
    builder: (context, data, reload) => _HoursForm(initial: data),
  );
}

class _HoursForm extends StatefulWidget {
  const _HoursForm({required this.initial});
  final Json initial;

  @override
  State<_HoursForm> createState() => _HoursFormState();
}

class _HoursFormState extends State<_HoursForm> {
  late final Set<int> _days = {
    for (final d in (widget.initial['weekdays'] as List? ?? const []))
      if (d is int) d,
  };
  late final _open = TextEditingController(
    text: widget.initial['open'] as String? ?? '09:00',
  );
  late final _close = TextEditingController(
    text: widget.initial['close'] as String? ?? '18:00',
  );
  bool _invalid = false;

  @override
  void dispose() {
    _open.dispose();
    _close.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final t = context.t, open = _open.text.trim(), close = _close.text.trim();
    if (!validBusinessHours(_days, open, close)) {
      setState(() => _invalid = true);
      return;
    }
    setState(() => _invalid = false);
    try {
      await context.api.put('/admin/setup/business-hours', {
        'weekdays': _days.toList()..sort(),
        'open': open,
        'close': close,
      });
      if (mounted) showToast(context, t.cSaved);
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final names = t.cRbDays.split(',');
    Widget time(TextEditingController c, String label) => SizedBox(
      width: 160,
      child: TextField(
        controller: c,
        decoration: InputDecoration(labelText: label, isDense: true),
      ),
    );
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(t.cRbHoursTitle, style: context.text.titleSmall),
          KeepAllText(t.cRbHoursHint, style: context.text.bodySmall),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.xs,
            runSpacing: Space.xs,
            children: [
              for (var d = 0; d < 7; d++)
                FilterChip(
                  label: KeepAllText(names[d]),
                  selected: _days.contains(d),
                  onSelected: (on) =>
                      setState(() => on ? _days.add(d) : _days.remove(d)),
                ),
            ],
          ),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.sm,
            runSpacing: Space.sm,
            children: [
              time(_open, t.cRbHoursOpen),
              time(_close, t.cRbHoursClose),
            ],
          ),
          if (_invalid)
            Padding(
              padding: const EdgeInsets.only(top: Space.xs),
              child: KeepAllText(
                t.cRbHoursInvalid,
                style: context.text.bodySmall?.copyWith(
                  color: context.colors.danger,
                ),
              ),
            ),
          const SizedBox(height: Space.sm),
          OutlinedButton(
            onPressed: context.console.isAdmin ? _save : null,
            child: KeepAllText(t.save),
          ),
        ],
      ),
    );
  }
}
