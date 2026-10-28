//! Two small custom widgets the stock ones can't stand in for:
//!
//! - [`Tile`]: a fixed-size rounded square that draws an image cropped to
//!   cover it (album art) or an icon centred inside it, or a gradient.
//!   `GtkPicture` would ask for the image's full size.
//! - [`StackView`]: holds the Popup cards and places each with a
//!   translate/scale transform, at a size set from outside. `GtkFixed`
//!   would grow with its transformed children mid-animation and resize the
//!   layer surface every frame.

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gdk, glib, graphene, gsk};

/// How a [`Tile`] fits its paintable.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Fit {
    /// Fill the tile, cropping what overflows.
    #[default]
    Cover,
    /// Fit inside a centred square of this side.
    Contain(f32),
}

/// Where a [`StackView`] child sits: its top-left corner, and a scale
/// about its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Default)]
    pub struct Tile {
        pub side: Cell<i32>,
        pub radius: Cell<f32>,
        pub fit: Cell<Fit>,
        pub paintable: RefCell<Option<gdk::Paintable>>,
        pub gradient: Cell<Option<(gdk::RGBA, gdk::RGBA)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Tile {
        const NAME: &'static str = "LucentTile";
        type Type = super::Tile;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Tile {}

    impl WidgetImpl for Tile {
        fn measure(&self, _: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let side = self.side.get();
            (side, side, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let (w, h) = (widget.width() as f32, widget.height() as f32);
            let bounds = graphene::Rect::new(0.0, 0.0, w, h);
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, self.radius.get()));
            if let Some((from, to)) = self.gradient.get() {
                snapshot.append_linear_gradient(
                    &bounds,
                    &graphene::Point::new(0.0, 0.0),
                    &graphene::Point::new(w, h),
                    &[gsk::ColorStop::new(0.0, from), gsk::ColorStop::new(1.0, to)],
                );
            }
            if let Some(paintable) = self.paintable.borrow().as_ref() {
                let (iw, ih) = match (paintable.intrinsic_width(), paintable.intrinsic_height()) {
                    (iw, ih) if iw > 0 && ih > 0 => (iw as f32, ih as f32),
                    _ => (w, h),
                };
                let scale = match self.fit.get() {
                    Fit::Cover => (w / iw).max(h / ih),
                    Fit::Contain(side) => (side / iw).min(side / ih),
                };
                let (dw, dh) = (iw * scale, ih * scale);
                snapshot.save();
                snapshot.translate(&graphene::Point::new((w - dw) / 2.0, (h - dh) / 2.0));
                paintable.snapshot(snapshot, f64::from(dw), f64::from(dh));
                snapshot.restore();
            }
            snapshot.pop();
        }
    }

    #[derive(Default)]
    pub struct StackView {
        pub size: Cell<(i32, i32)>,
        pub child_width: Cell<i32>,
        pub children: RefCell<Vec<(gtk4::Widget, Placement)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StackView {
        const NAME: &'static str = "LucentStackView";
        type Type = super::StackView;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for StackView {
        fn dispose(&self) {
            for (child, _) in self.children.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for StackView {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let (w, h) = self.size.get();
            let side = match orientation {
                gtk4::Orientation::Horizontal => w,
                _ => h,
            };
            (side, side, -1, -1)
        }

        fn size_allocate(&self, _: i32, _: i32, _: i32) {
            let width = self.child_width.get();
            for (child, place) in self.children.borrow().iter() {
                let height = child.measure(gtk4::Orientation::Vertical, width).1;
                let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
                let transform = gsk::Transform::new()
                    .translate(&graphene::Point::new(place.x + cx, place.y + cy))
                    .scale(place.scale, place.scale)
                    .translate(&graphene::Point::new(-cx, -cy));
                child.allocate(width, height, -1, Some(transform));
            }
        }
    }
}

glib::wrapper! {
    pub struct Tile(ObjectSubclass<imp::Tile>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Tile {
    /// A `side`-pixel tile with rounded corners.
    pub fn new(side: i32, radius: f64) -> Self {
        let tile: Self = glib::Object::new();
        tile.imp().side.set(side);
        tile.imp().radius.set(radius as f32);
        tile
    }

    pub fn set_paintable(&self, paintable: &impl IsA<gdk::Paintable>, fit: Fit) {
        self.imp()
            .paintable
            .replace(Some(paintable.clone().upcast()));
        self.imp().fit.set(fit);
        self.queue_draw();
    }

    /// Fill the tile with a diagonal gradient (top-left to bottom-right).
    pub fn set_gradient(&self, from: gdk::RGBA, to: gdk::RGBA) {
        self.imp().gradient.set(Some((from, to)));
        self.queue_draw();
    }
}

glib::wrapper! {
    pub struct StackView(ObjectSubclass<imp::StackView>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl StackView {
    /// A view whose children are all `child_width` wide.
    pub fn new(child_width: i32) -> Self {
        let view: Self = glib::Object::new();
        view.imp().child_width.set(child_width);
        view
    }

    pub fn set_child_width(&self, width: i32) {
        if self.imp().child_width.replace(width) != width {
            self.queue_allocate();
        }
    }

    /// Set the size the view asks for, independent of its children.
    pub fn set_size(&self, width: i32, height: i32) {
        if self.imp().size.replace((width, height)) != (width, height) {
            self.queue_resize();
        }
    }

    /// Add a child on top of the others.
    pub fn add(&self, child: &impl IsA<gtk4::Widget>, place: Placement) {
        child.set_parent(self);
        self.imp()
            .children
            .borrow_mut()
            .push((child.clone().upcast(), place));
        self.queue_allocate();
    }

    pub fn remove(&self, child: &impl IsA<gtk4::Widget>) {
        let child = child.as_ref();
        self.imp().children.borrow_mut().retain(|(c, _)| c != child);
        child.unparent();
        self.queue_allocate();
    }

    pub fn place(&self, child: &impl IsA<gtk4::Widget>, place: Placement) {
        let child = child.as_ref();
        if let Some(entry) = self
            .imp()
            .children
            .borrow_mut()
            .iter_mut()
            .find(|(c, _)| c == child)
        {
            entry.1 = place;
        }
        self.queue_allocate();
    }

    /// A child's height at the view's child width.
    pub fn child_height(&self, child: &impl IsA<gtk4::Widget>) -> i32 {
        child
            .as_ref()
            .measure(gtk4::Orientation::Vertical, self.imp().child_width.get())
            .1
    }
}
