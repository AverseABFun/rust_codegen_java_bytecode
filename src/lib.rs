#![allow(internal_features)]
#![feature(rustc_private)]

#[allow(unused_imports)]
#[macro_use]
extern crate rustc_middle;
extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_codegen_ssa;
extern crate rustc_data_structures;
extern crate rustc_errors;
extern crate rustc_fluent_macro;
extern crate rustc_fs_util;
extern crate rustc_hir;
extern crate rustc_incremental;
extern crate rustc_index;
extern crate rustc_metadata;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_stable_hash;
extern crate rustc_target;
#[macro_use]
extern crate tracing;

// This prevents duplicating functions and statics that are already part of the host rustc process.
#[allow(unused_extern_crates)]
extern crate rustc_driver;

rustc_fluent_macro::fluent_messages! { "../messages.ftl" }

use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::Mutex;

use rustc_codegen_ssa::back::lto::{LtoModuleCodegen, SerializedModule, ThinModule};
use rustc_codegen_ssa::back::write::{CodegenContext, FatLtoInput, ModuleConfig, OngoingCodegen};
use rustc_codegen_ssa::base::codegen_crate;
use rustc_codegen_ssa::traits::{
    CodegenBackend, ExtraBackendMethods, ModuleBufferMethods, ThinBufferMethods,
    WriteBackendMethods,
};
use rustc_codegen_ssa::{CodegenResults, CompiledModule, ModuleCodegen};
use rustc_data_structures::fx::FxIndexMap;
use rustc_data_structures::sync::IntoDynSyncSend;
use rustc_errors::{DiagCtxtHandle, FatalError};
use rustc_metadata::EncodedMetadata;
use rustc_middle::dep_graph::{WorkProduct, WorkProductId};
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_session::config::{OutputFilenames, OutputType};

use rustc_hir::def_id::LOCAL_CRATE;

fn arch_to_gcc(name: &str) -> &str {
    match name {
        "M68020" => "68020",
        _ => name,
    }
}

fn handle_native(name: &str) -> &str {
    if name != "native" {
        return arch_to_gcc(name);
    }

    std::env::consts::ARCH
}

pub fn target_cpu(sess: &Session) -> &str {
    match sess.opts.cg.target_cpu {
        Some(ref name) => handle_native(name),
        Option::None => handle_native(sess.target.cpu.as_ref()),
    }
}

struct CodegenData {}

#[derive(Debug)]
pub struct TargetInfo {
    pub target_cpu: String,
}

#[derive(Clone)]
pub struct LockedTargetInfo {
    // this and impls for it were copied from rustc_codegen_gcc
    info: Arc<Mutex<IntoDynSyncSend<TargetInfo>>>,
}

impl Debug for LockedTargetInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.info.lock().expect("lock").fmt(formatter)
    }
}

#[derive(Clone)]
struct JavaBytecodeBackend {
    target_info: Option<LockedTargetInfo>,
}

impl CodegenBackend for JavaBytecodeBackend {
    fn locale_resource(&self) -> &'static str {
        crate::DEFAULT_LOCALE_RESOURCE
    }

    fn codegen_crate<'a, 'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        metadata: EncodedMetadata,
        need_metadata_module: bool,
    ) -> Box<dyn Any> {
        info!("codegen crate {}", tcx.crate_name(LOCAL_CRATE));

        let target_cpu = target_cpu(tcx.sess).to_string();

        let mut backend = self.clone();

        backend.target_info = Some(LockedTargetInfo {
            info: Arc::new(Mutex::new(IntoDynSyncSend(TargetInfo {
                target_cpu: target_cpu.clone(),
            }))),
        });

        Box::new(codegen_crate(
            backend,
            tcx,
            target_cpu,
            metadata,
            need_metadata_module,
        ))
    }

    fn join_codegen(
        &self,
        ongoing_codegen: Box<dyn Any>,
        _sess: &Session,
        outputs: &OutputFilenames,
    ) -> (CodegenResults, FxIndexMap<WorkProductId, WorkProduct>) {
        let ongoing_codegen: Box<OngoingCodegen<Self>> = ongoing_codegen.downcast().unwrap();
        for (output_type, filename) in outputs.outputs.iter() {
            match *output_type {
                OutputType::
            }
        }
    }

    fn link(&self, sess: &Session, codegen_results: CodegenResults, outputs: &OutputFilenames) {
        unimplemented!();
    }
}

impl ExtraBackendMethods for JavaBytecodeBackend {
    fn codegen_allocator<'tcx>(
        &self,
        _tcx: TyCtxt<'tcx>,
        _module_name: &str,
        _kind: rustc_ast::expand::allocator::AllocatorKind,
        _alloc_error_handler_kind: rustc_ast::expand::allocator::AllocatorKind,
    ) -> Self::Module {
        unimplemented!();
    }

    fn compile_codegen_unit(
        &self,
        _tcx: TyCtxt<'_>,
        _cgu_name: rustc_span::Symbol,
    ) -> (rustc_codegen_ssa::ModuleCodegen<Self::Module>, u64) {
        unimplemented!()
    }

    fn target_machine_factory(
        &self,
        _sess: &Session,
        _opt_level: rustc_session::config::OptLevel,
        _target_features: &[String],
    ) -> rustc_codegen_ssa::back::write::TargetMachineFactoryFn<Self> {
        unimplemented!()
    }
}

struct ModuleBuffer {
    data: Vec<u8>,
}

impl ModuleBufferMethods for ModuleBuffer {
    fn data(&self) -> &[u8] {
        &self.data
    }
}

struct ThinBuffer {
    data: Vec<u8>,
    thin_link_data: Vec<u8>,
}

impl ThinBufferMethods for ThinBuffer {
    fn data(&self) -> &[u8] {
        &self.data
    }
    fn thin_link_data(&self) -> &[u8] {
        &self.thin_link_data
    }
}

impl WriteBackendMethods for JavaBytecodeBackend {
    type Module = CodegenData;
    type TargetMachine = TargetInfo;
    type TargetMachineError = ();
    type ModuleBuffer = ModuleBuffer;
    type ThinData = ();
    type ThinBuffer = ThinBuffer;

    fn run_fat_lto(
        _cgcx: &CodegenContext<Self>,
        _modules: Vec<FatLtoInput<Self>>,
        _cached_modules: Vec<(SerializedModule<Self::ModuleBuffer>, WorkProduct)>,
    ) -> Result<LtoModuleCodegen<Self>, FatalError> {
        unimplemented!();
    }

    fn run_thin_lto(
        _cgcx: &CodegenContext<Self>,
        _modules: Vec<(String, Self::ThinBuffer)>,
        _cached_modules: Vec<(SerializedModule<Self::ModuleBuffer>, WorkProduct)>,
    ) -> Result<(Vec<LtoModuleCodegen<Self>>, Vec<WorkProduct>), FatalError> {
        unimplemented!();
    }

    fn print_pass_timings(&self) {
        unimplemented!();
    }

    fn print_statistics(&self) {
        unimplemented!()
    }

    unsafe fn optimize(
        _cgcx: &CodegenContext<Self>,
        _dcx: DiagCtxtHandle<'_>,
        _module: &ModuleCodegen<Self::Module>,
        _config: &ModuleConfig,
    ) -> Result<(), FatalError> {
        unimplemented!();
    }

    fn optimize_fat(
        _cgcx: &CodegenContext<Self>,
        _module: &mut ModuleCodegen<Self::Module>,
    ) -> Result<(), FatalError> {
        unimplemented!();
    }

    unsafe fn optimize_thin(
        _cgcx: &CodegenContext<Self>,
        _thin: ThinModule<Self>,
    ) -> Result<ModuleCodegen<Self::Module>, FatalError> {
        unimplemented!();
    }

    unsafe fn codegen(
        _cgcx: &CodegenContext<Self>,
        _dcx: DiagCtxtHandle<'_>,
        _module: ModuleCodegen<Self::Module>,
        _config: &ModuleConfig,
    ) -> Result<CompiledModule, FatalError> {
        unimplemented!();
    }

    fn prepare_thin(
        _module: ModuleCodegen<Self::Module>,
        _emit_summary: bool,
    ) -> (String, Self::ThinBuffer) {
        unimplemented!();
    }

    fn serialize_module(_module: ModuleCodegen<Self::Module>) -> (String, Self::ModuleBuffer) {
        unimplemented!();
    }

    fn run_link(
        _cgcx: &CodegenContext<Self>,
        _dcx: DiagCtxtHandle<'_>,
        _modules: Vec<ModuleCodegen<Self::Module>>,
    ) -> Result<ModuleCodegen<Self::Module>, FatalError> {
        unimplemented!();
    }
    fn autodiff(
        _cgcx: &CodegenContext<Self>,
        _tcx: TyCtxt<'_>,
        _module: &ModuleCodegen<Self::Module>,
        _diff_fncs: Vec<rustc_ast::expand::autodiff_attrs::AutoDiffItem>,
        _config: &ModuleConfig,
    ) -> Result<(), FatalError> {
        unimplemented!();
    }
}

#[unsafe(no_mangle)]
pub fn __rustc_codegen_backend() -> Box<dyn CodegenBackend> {
    Box::new(JavaBytecodeBackend { target_info: None })
}
