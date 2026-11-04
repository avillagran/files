//! Extraction uses the existing destination modal, AuxIo and global queue.
use crate::{copy_dialog::Dialog, copy_job::Request, model::Model};
use files_core::copy_engine::Control;

impl Model {
    pub fn open_create_archive(&mut self) {
        if self.archive_preparing.is_some()
            || self.move_preparing.is_some()
            || self.mutation_applied
        {
            self.error = "Espera la revisión pendiente antes de crear ZIP".into();
            return;
        }
        self.open_copy_mode(false);
        let Some(form) = self.copy_form.as_mut() else {
            return;
        };
        if form.names.is_empty() || form.names.len() > 128 || form.multi.is_some() {
            self.copy_form = None;
            self.error = "Crear ZIP: selecciona hasta128 raíces de una carpeta".into();
            return;
        }
        let mut name = if form.names.len() == 1 {
            form.names[0].clone()
        } else {
            "Archivos".into()
        };
        name.push(".zip");
        let destination = form.destination_path().join(name);
        form.extracting = true;
        form.creating = true;
        form.capture_destination(destination);
        form.touch();
        self.hits.clear();
    }
    pub fn open_extract(&mut self) {
        if self.archive_preparing.is_some()
            || self.move_preparing.is_some()
            || self.mutation_applied
        {
            self.error = "Espera la revisión pendiente antes de extraer".into();
            return;
        }
        let count = self
            .selection
            .len()
            .max(usize::from(self.selected.is_some()));
        if count != 1 {
            self.error = "Selecciona un ZIP para extraer en una carpeta nueva".into();
            return;
        }
        self.open_copy_mode(false);
        let Some(form) = self.copy_form.as_mut() else {
            return;
        };
        if form.names.len() != 1 {
            self.copy_form = None;
            return;
        }
        let mut name = std::path::Path::new(&form.names[0])
            .file_stem()
            .unwrap_or(&form.names[0])
            .to_os_string();
        name.push("-extraido");
        let destination = form.destination_path().join(name);
        form.extracting = true;
        form.capture_destination(destination);
        form.touch();
        self.hits.clear();
    }
    pub fn start_extract(&mut self, mut form: Dialog) {
        if form.preparing || self.closing {
            form.error = "Espera la revisión o cancela".into();
            self.copy_form = Some(form);
            return;
        }
        self.copy.set_external_fds(
            self.clipboard_state.external_fds()
                + self.native_transfer.external_fds()
                + self.search_transfer_fds(),
        );
        if let Some(prepared) = form.archive.take() {
            let id = form.prepare_id.expect("ZIP reviewed id");
            let destination = prepared.paths()[0]
                .destination
                .parent()
                .expect("new child")
                .to_path_buf();
            let request = Request::reviewed_extract(id, prepared);
            let request = if form.start_paused {
                request.start_paused()
            } else {
                request
            };
            match self.copy.enqueue(request) {
                Ok(()) => {
                    self.copy_owners.insert(id, self.pane_owner());
                    self.copy_locations.insert(id, destination);
                    self.operation_message = Some(format!(
                        "{} ZIP #{id} encolada",
                        if form.creating {
                            "Creación"
                        } else {
                            "Extracción"
                        }
                    ));
                    self.can_undo = false;
                    self.can_redo = false;
                }
                Err(mut rejected) => {
                    form.archive = rejected.request.take_archive();
                    form.error = format!(
                        "{} · {} bytes disponibles",
                        rejected.reason,
                        self.copy.budget().available_bytes
                    );
                    self.copy_form = Some(form);
                }
            }
            return;
        }
        if self.archive_preparing.is_some() || self.move_preparing.is_some() {
            form.error = "Una revisión sigue pendiente".into();
            self.copy_form = Some(form);
            return;
        }
        let destination = form.destination_path();
        let target = destination
            .parent()
            .zip(destination.file_name())
            .filter(|(parent, _)| parent.is_absolute());
        let Some((parent, name)) = target else {
            form.error = "Usa ruta absoluta de salida nueva (padre existente)".into();
            self.copy_form = Some(form);
            return;
        };
        if self
            .copy
            .fd_reservation()
            .saturating_add(if form.creating { 76 } else { 8 })
            > 256
        {
            form.error = "Espera: revisión y cola superan256 descriptores".into();
            self.copy_form = Some(form);
            return;
        }
        self.copy_id = self
            .copy_id
            .checked_add(1)
            .expect("operation ids exhausted");
        let id = self.copy_id;
        let control = Control::default();
        let request = if form.creating {
            crate::aux_io::Request::PrepareCreateArchive(
                id,
                form.source.clone(),
                form.names.to_vec(),
                parent.into(),
                name.into(),
                control.clone(),
                self.copy.budget().available_bytes,
            )
        } else {
            crate::aux_io::Request::PrepareArchive(
                id,
                form.source.join(&form.names[0]),
                parent.into(),
                name.into(),
                control.clone(),
                self.copy.budget().available_bytes,
            )
        };
        match self.aux.tx.try_send(request) {
            Ok(()) => {
                self.archive_preparing = Some((id, control));
                form.prepare_id = Some(id);
                form.preparing = true;
                form.error.clear();
                form.touch();
            }
            Err(_) => form.error = "Trabajador ocupado; reintenta la revisión".into(),
        }
        self.copy_form = Some(form);
    }
    pub fn receive_archive(
        &mut self,
        id: u64,
        result: Result<crate::archive_operation::Prepared, String>,
    ) {
        let Some((pending, control)) = &self.archive_preparing else {
            return;
        };
        if *pending != id {
            return;
        }
        let cancelled = control.is_cancelled();
        self.archive_preparing = None;
        if self.closing {
            return;
        }
        let Some(form) = self
            .copy_form
            .as_mut()
            .filter(|d| d.extracting && d.prepare_id == Some(id))
        else {
            return;
        };
        form.preparing = false;
        form.touch();
        if cancelled {
            form.error = "Revisión cancelada; vuelve a revisar".into();
            return;
        }
        match result {
            Ok(prepared) => {
                form.archive = Some(prepared);
                form.error.clear();
            }
            Err(error) => {
                form.prepare_id = None;
                form.error = error;
            }
        }
        self.hits.clear();
    }
    pub fn archive_review_worker_lost(&mut self) -> bool {
        let Some((id, control)) = self.archive_preparing.take() else {
            return false;
        };
        control.cancel();
        if let Some(form) = self
            .copy_form
            .as_mut()
            .filter(|d| d.extracting && d.prepare_id == Some(id))
        {
            form.preparing = false;
            form.prepare_id = None;
            form.error = "Trabajador terminó; revisión ZIP sin confirmar".into();
            form.touch();
        }
        true
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
