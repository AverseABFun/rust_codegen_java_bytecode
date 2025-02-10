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

use rustc_codegen_ssa::back::lto::{LtoModuleCodegen, SerializedModule, ThinModule};
use rustc_codegen_ssa::back::write::{CodegenContext, FatLtoInput, ModuleConfig};
use rustc_codegen_ssa::base::codegen_crate;
use rustc_codegen_ssa::traits::{CodegenBackend, ExtraBackendMethods, WriteBackendMethods};
use rustc_codegen_ssa::{CodegenResults, CompiledModule, ModuleCodegen};
use rustc_data_structures::fx::FxIndexMap;
use rustc_errors::{DiagCtxtHandle, FatalError};
use rustc_metadata::EncodedMetadata;
use rustc_middle::bug;
use rustc_middle::dep_graph::{WorkProduct, WorkProductId};
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_session::config::OutputFilenames;

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

#[derive(Clone)]
pub struct LockedTargetInfo { // this and impls for it were copied from rustc_codegen_gcc
    info: Arc<Mutex<IntoDynSyncSend<TargetInfo>>>,
}

impl Debug for LockedTargetInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.info.lock().expect("lock").fmt(formatter)
    }
}

impl LockedTargetInfo {
    fn cpu_supports(&self, feature: &str) -> bool {
        self.info.lock().expect("lock").cpu_supports(feature)
    }

    fn supports_target_dependent_type(&self, typ: CType) -> bool {
        self.info.lock().expect("lock").supports_target_dependent_type(typ)
    }
}

#[derive(Clone)]
struct JavaBytecodeBackend {
    target_info: LockedTargetInfo
};

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

        Box::new(codegen_crate(
            self,
            tcx,
            target_cpu.to_string(),
            metadata,
            need_metadata_module,
        ))
    }

    fn join_codegen(
        &self,
        ongoing_codegen: Box<dyn Any>,
        _sess: &Session,
        _outputs: &OutputFilenames,
    ) -> (CodegenResults, FxIndexMap<WorkProductId, WorkProduct>) {
        let codegen_results = ongoing_codegen
            .downcast::<CodegenResults>()
            .expect("in join_codegen: ongoing_codegen is not a CodegenResults");
        (*codegen_results, FxIndexMap::default())
    }

    fn link(&self, sess: &Session, codegen_results: CodegenResults, outputs: &OutputFilenames) {
        use std::io::Write;

        use rustc_session::config::{CrateType, OutFileName};
        use rustc_session::output::out_filename;

        let crate_name = codegen_results.crate_info.local_crate_name;
        for &crate_type in sess.opts.crate_types.iter() {
            if crate_type != CrateType::Rlib {
                sess.dcx().fatal(format!("Crate type is {:?}", crate_type));
            }
            let output_name = out_filename(sess, crate_type, &outputs, crate_name);
            match output_name {
                OutFileName::Real(ref path) => {
                    let mut out_file = ::std::fs::File::create(path).unwrap();
                    writeln!(out_file, "This has been 'compiled' successfully.").unwrap();
                }
                OutFileName::Stdout => {
                    let mut stdout = std::io::stdout();
                    writeln!(stdout, "This has been 'compiled' successfully.").unwrap();
                }
            }
        }
    }
}

impl ExtraBackendMethods for JavaBytecodeBackend {
    fn codegen_allocator<'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        module_name: &str,
        kind: rustc_ast::expand::allocator::AllocatorKind,
        alloc_error_handler_kind: rustc_ast::expand::allocator::AllocatorKind,
    ) -> Self::Module {
        CodegenData {}
    }

    fn compile_codegen_unit(
        &self,
        tcx: TyCtxt<'_>,
        cgu_name: rustc_span::Symbol,
    ) -> (rustc_codegen_ssa::ModuleCodegen<Self::Module>, u64) {
        base::compile_codegen_unit(tcx, cgu_name, self.target_info.clone())
    }

    fn target_machine_factory(
        &self,
        sess: &Session,
        opt_level: rustc_session::config::OptLevel,
        target_features: &[String],
    ) -> rustc_codegen_ssa::back::write::TargetMachineFactoryFn<Self> {
    }
}

impl WriteBackendMethods for JavaBytecodeBackend {
    type Module = CodegenData;
    type TargetMachine = ();
    type TargetMachineError = ();
    type ModuleBuffer = ();
    type ThinData = ();
    type ThinBuffer = ();

    fn run_fat_lto(
        cgcx: &CodegenContext<Self>,
        modules: Vec<FatLtoInput<Self>>,
        cached_modules: Vec<(SerializedModule<Self::ModuleBuffer>, WorkProduct)>,
    ) -> Result<LtoModuleCodegen<Self>, FatalError> {
        unimplemented!();
    }

    fn run_thin_lto(
        cgcx: &CodegenContext<Self>,
        modules: Vec<(String, Self::ThinBuffer)>,
        cached_modules: Vec<(SerializedModule<Self::ModuleBuffer>, WorkProduct)>,
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
        module: &ModuleCodegen<Self::Module>,
        config: &ModuleConfig,
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
        cgcx: &CodegenContext<Self>,
        thin: ThinModule<Self>,
    ) -> Result<ModuleCodegen<Self::Module>, FatalError> {
        unimplemented!();
    }

    unsafe fn codegen(
        cgcx: &CodegenContext<Self>,
        dcx: DiagCtxtHandle<'_>,
        module: ModuleCodegen<Self::Module>,
        config: &ModuleConfig,
    ) -> Result<CompiledModule, FatalError> {
        unimplemented!();
    }

    fn prepare_thin(
        module: ModuleCodegen<Self::Module>,
        emit_summary: bool,
    ) -> (String, Self::ThinBuffer) {
        unimplemented!();
    }

    fn serialize_module(_module: ModuleCodegen<Self::Module>) -> (String, Self::ModuleBuffer) {
        unimplemented!();
    }

    fn run_link(
        cgcx: &CodegenContext<Self>,
        dcx: DiagCtxtHandle<'_>,
        modules: Vec<ModuleCodegen<Self::Module>>,
    ) -> Result<ModuleCodegen<Self::Module>, FatalError> {
        unimplemented!();
    }
    fn autodiff(
        cgcx: &CodegenContext<Self>,
        tcx: TyCtxt<'_>,
        module: &ModuleCodegen<Self::Module>,
        diff_fncs: Vec<rustc_ast::expand::autodiff_attrs::AutoDiffItem>,
        config: &ModuleConfig,
    ) -> Result<(), FatalError> {
        unimplemented!();
    }
}

#[unsafe(no_mangle)]
pub fn __rustc_codegen_backend() -> Box<dyn CodegenBackend> {
    Box::new(JavaBytecodeBackend)
}
