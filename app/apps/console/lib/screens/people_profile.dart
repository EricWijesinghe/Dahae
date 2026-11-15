import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:intl/intl.dart';
import 'package:ui/ui.dart';

import '../state.dart';

/// Opens the verification checklist and employee profile of one pro. Approving a pro as active needs
/// every checklist item ticked; the server enforces the same rule.
Future<void> showProProfile(
  BuildContext context,
  String proId,
  Future<void> Function() reload,
) {
  return showDialog<void>(
    context: context,
    builder: (ctx) => Dialog(
      child: SizedBox(
        width: 520,
        height: 580,
        child: AsyncView<Json>(
          load: () async =>
              (await ctx.api.get('/admin/pros/$proId/profile')) as Json,
          builder: (context, data, _) =>
              _ProfileTabs(proId: proId, data: data, reload: reload),
        ),
      ),
    ),
  );
}

class _ProfileTabs extends StatelessWidget {
  const _ProfileTabs({
    required this.proId,
    required this.data,
    required this.reload,
  });
  final String proId;
  final Json data;
  final Future<void> Function() reload;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return DefaultTabController(
      length: 2,
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(Space.lg),
            child: Text(t.cPfTitle, style: context.text.titleSmall),
          ),
          TabBar(
            tabs: [
              Tab(text: t.cPfTabChecklist),
              Tab(text: t.cPfTabProfile),
            ],
          ),
          Expanded(
            child: TabBarView(
              children: [
                _ChecklistTab(proId: proId, data: data, reload: reload),
                _ProfileTab(proId: proId, data: data),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

String _itemLabel(BuildContext context, String key) {
  final t = context.t;
  return switch (key) {
    'identity' => t.cPfItemIdentity,
    'tax_type' => t.cPfItemTaxType,
    'experience' => t.cPfItemExperience,
    'emergency_contact' => t.cPfItemEmergency,
    _ => key,
  };
}

class _ChecklistTab extends StatefulWidget {
  const _ChecklistTab({
    required this.proId,
    required this.data,
    required this.reload,
  });
  final String proId;
  final Json data;
  final Future<void> Function() reload;

  @override
  State<_ChecklistTab> createState() => _ChecklistTabState();
}

class _ChecklistTabState extends State<_ChecklistTab> {
  late final Map<String, bool> _checks;
  late final String _status = widget.data['status'] as String;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    final saved = (widget.data['checklist'] as Map?) ?? const {};
    _checks = {
      for (final k in (widget.data['items'] as List).cast<String>())
        k: saved[k] == true,
    };
  }

  bool get _complete => _checks.values.every((v) => v);

  Future<void> _send(String status) async {
    final t = context.t;
    setState(() => _saving = true);
    try {
      await context.api.post('/admin/pros/${widget.proId}/status', {
        'status': status,
        'checklist': _checks,
      });
      if (!mounted) return;
      showToast(context, t.cDone);
      await widget.reload();
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _approve() async {
    if (!_complete) {
      showToast(context, context.t.cPfIncomplete);
      return;
    }
    await _send('verified');
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return ListView(
      padding: const EdgeInsets.all(Space.lg),
      children: [
        Text(t.cPfChecklistHelp),
        if (_status == 'verified' && widget.data['decided_at'] == null)
          Padding(
            padding: const EdgeInsets.only(top: Space.sm),
            child: Text(t.cPfLegacyNote),
          ),
        for (final k in _checks.keys)
          CheckboxListTile(
            value: _checks[k],
            title: Text(_itemLabel(context, k)),
            onChanged: _saving
                ? null
                : (v) => setState(() => _checks[k] = v ?? false),
          ),
        const SizedBox(height: Space.sm),
        Wrap(
          spacing: Space.sm,
          children: [
            OutlinedButton(
              onPressed: _saving ? null : () => _send(_status),
              child: Text(t.cPfSaveChecklist),
            ),
            FilledButton(
              onPressed: _saving || _status == 'verified' ? null : _approve,
              child: Text(t.cPfApprove),
            ),
          ],
        ),
      ],
    );
  }
}

class _ProfileTab extends StatefulWidget {
  const _ProfileTab({required this.proId, required this.data});
  final String proId;
  final Json data;

  @override
  State<_ProfileTab> createState() => _ProfileTabState();
}

class _ProfileTabState extends State<_ProfileTab> {
  late final Json _p =
      (widget.data['profile'] as Map?)?.cast<String, dynamic>() ?? const {};
  late final _bio = TextEditingController(text: _p['bio'] as String? ?? '');
  late final _contact = TextEditingController(
    text: _p['emergency_contact'] as String? ?? '',
  );
  late String? _photo = _p['photo_url'] as String?;
  late String? _employment = _p['employment_type'] as String?;
  late DateTime? _hired = DateTime.tryParse(_p['hired_on'] as String? ?? '');
  bool _saving = false;

  @override
  void dispose() {
    _bio.dispose();
    _contact.dispose();
    super.dispose();
  }

  Future<void> _pickPhoto() async {
    final t = context.t;
    final file = await ImagePicker().pickImage(
      source: ImageSource.gallery,
      maxWidth: 1024,
    );
    if (file == null || !mounted) return;
    setState(() => _saving = true);
    try {
      final url = await context.api.upload(await file.readAsBytes(), file.name);
      if (mounted) setState(() => _photo = url);
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _pickDate() async {
    final now = DateTime.now();
    final picked = await showDatePicker(
      context: context,
      initialDate: _hired ?? now,
      firstDate: DateTime(2000),
      lastDate: now,
    );
    if (picked != null && mounted) setState(() => _hired = picked);
  }

  Future<void> _save() async {
    final t = context.t;
    setState(() => _saving = true);
    try {
      await context.api.put('/admin/pros/${widget.proId}/profile', {
        'photo_url': _photo,
        'bio': _bio.text,
        'employment_type': _employment,
        'emergency_contact': _contact.text,
        'hired_on': _hired == null
            ? null
            : DateFormat('yyyy-MM-dd').format(_hired!),
      });
      if (mounted) showToast(context, t.cDone);
    } catch (e) {
      if (mounted) showToast(context, t.error(e));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return ListView(
      padding: const EdgeInsets.all(Space.lg),
      children: [
        OutlinedButton.icon(
          onPressed: _saving ? null : _pickPhoto,
          icon: Icon(_photo == null ? Icons.add_a_photo_outlined : Icons.check),
          label: Text(t.cPfPickPhoto),
        ),
        const SizedBox(height: Space.sm),
        TextField(
          controller: _bio,
          maxLines: 3,
          maxLength: 500,
          decoration: InputDecoration(labelText: t.cPfBio),
        ),
        const SizedBox(height: Space.sm),
        SegmentedButton<String>(
          emptySelectionAllowed: true,
          segments: [
            ButtonSegment(value: 'freelance', label: Text(t.cPfEmpFreelance)),
            ButtonSegment(value: 'employee', label: Text(t.cPfEmpEmployee)),
            ButtonSegment(value: 'contract', label: Text(t.cPfEmpContract)),
          ],
          selected: {?_employment},
          onSelectionChanged: (s) =>
              setState(() => _employment = s.isEmpty ? null : s.first),
        ),
        const SizedBox(height: Space.sm),
        TextField(
          controller: _contact,
          keyboardType: TextInputType.phone,
          decoration: InputDecoration(
            labelText: t.cPfEmergency,
            helperText: t.cPfEmergencyHelp,
          ),
        ),
        ListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(t.cPfHiredOn),
          subtitle: Text(
            _hired == null
                ? t.cPfNoDate
                : DateFormat('yyyy-MM-dd').format(_hired!),
          ),
          trailing: const Icon(Icons.calendar_today_outlined),
          onTap: _saving ? null : _pickDate,
        ),
        const SizedBox(height: Space.sm),
        FilledButton(
          onPressed: _saving ? null : _save,
          child: Text(t.cPfSaveProfile),
        ),
      ],
    );
  }
}
