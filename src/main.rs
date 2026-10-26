mod analysis;
mod app;
mod eval;
mod parser;
mod plotting;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Calcolatrice")
            .with_app_id("calcolatrice")
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([760.0, 500.0])
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native("Calcolatrice", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
