#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Operation(u32);

impl Operation {
    // Kirikiri
    pub const COPY: Self = Self(0);
    pub const FILL_ARGB: Self = Self(1);
    pub const FILL_COLOR: Self = Self(2);
    pub const FILL_MASK: Self = Self(3);
    pub const COPY_COLOR: Self = Self(4);
    pub const COPY_MASK: Self = Self(5);
    pub const COPY_OPAQUE_IMAGE: Self = Self(6);
    pub const ALPHA_BLEND: Self = Self(7);
    pub const CONST_ALPHA_BLEND: Self = Self(8);
    pub const CONST_COLOR_ALPHA_BLEND: Self = Self(9);
    pub const APPLY_COLOR_MAP: Self = Self(10);
    pub const REMOVE_CONST_OPACITY: Self = Self(11);
    pub const REMOVE_OPACITY: Self = Self(12);
    pub const CONST_ALPHA_BLEND_SD: Self = Self(13);
    pub const UNIV_TRANS_BLEND: Self = Self(14);
    pub const ALPHA_TO_ADDITIVE_ALPHA: Self = Self(15);
    pub const ADDITIVE_ALPHA_TO_ALPHA: Self = Self(16);
    pub const DO_GRAY_SCALE: Self = Self(17);
    pub const BOX_BLUR: Self = Self(18);
    pub const ADJUST_GAMMA: Self = Self(19);
    pub const PS_BLEND: Self = Self(20);
    pub const NON_PS_BLEND: Self = Self(21);
    pub const BOX_BLUR_H: Self = Self(40);
    pub const BOX_BLUR_V: Self = Self(41);

    // Video
    pub const VIDEO_COLOR: Self = Self(22);
    pub const LAYER_MOVIE_ALPHA: Self = Self(30);

    // LayerExText
    pub const LAYER_EX_TEXT_PAINT: Self = Self(35);

    // LayerExRaster
    pub const LAYER_EX_RASTER: Self = Self(31);

    // LayerExImage
    pub const LAYER_EX_LIGHT: Self = Self(23);
    pub const LAYER_EX_COLORIZE: Self = Self(24);
    pub const LAYER_EX_MODULATE: Self = Self(25);
    pub const LAYER_EX_NOISE: Self = Self(26);
    pub const LAYER_EX_WHITE_NOISE: Self = Self(27);
    pub const LAYER_EX_GAUSSIAN_H: Self = Self(28);
    pub const LAYER_EX_GAUSSIAN_V: Self = Self(29);

    // extrans
    pub const EXTRANS_WAVE: Self = Self(32);
    pub const EXTRANS_MOSAIC: Self = Self(33);
    pub const EXTRANS_BLUR_FADE: Self = Self(34);
    pub const EXTRANS_TURN: Self = Self(36);
    pub const EXTRANS_ROTATE: Self = Self(37);
    pub const EXTRANS_RIPPLE: Self = Self(38);

    // extNagano
    pub const EXT_NAGANO: Self = Self(39);

    /// Returns the opcode; unknown methods fall back to `COPY`.
    /// Alpha variants and blend modes use separate shader parameters.
    pub const fn to_op(method: &str) -> Self {
        let method = match method.as_bytes() {
            [base @ .., b'_', b'H', b'D', b'A'] => base,
            base => base,
        };
        let method = match method {
            [base @ .., b'_', b'd'] | [base @ .., b'_', b'a'] => base,
            base => base,
        };
        match method {
            b"Copy" => Self::COPY,
            b"FillARGB" => Self::FILL_ARGB,
            b"FillColor" => Self::FILL_COLOR,
            b"FillMask" => Self::FILL_MASK,
            b"CopyColor" => Self::COPY_COLOR,
            b"CopyMask" => Self::COPY_MASK,
            b"CopyOpaqueImage" => Self::COPY_OPAQUE_IMAGE,
            b"AlphaBlend" | b"PerspectiveAlphaBlend" | b"AdditiveAlphaBlend" => Self::ALPHA_BLEND,
            b"ConstAlphaBlend" => Self::CONST_ALPHA_BLEND,
            b"ConstColorAlphaBlend" => Self::CONST_COLOR_ALPHA_BLEND,
            b"ApplyColorMap" => Self::APPLY_COLOR_MAP,
            b"RemoveConstOpacity" => Self::REMOVE_CONST_OPACITY,
            b"RemoveOpacity" => Self::REMOVE_OPACITY,
            b"ConstAlphaBlend_SD" | b"AlphaBlend_SD" => Self::CONST_ALPHA_BLEND_SD,
            b"UnivTransBlend" => Self::UNIV_TRANS_BLEND,
            b"AlphaToAdditiveAlpha" => Self::ALPHA_TO_ADDITIVE_ALPHA,
            b"AdditiveAlphaToAlpha" => Self::ADDITIVE_ALPHA_TO_ALPHA,
            b"DoGrayScale" => Self::DO_GRAY_SCALE,
            b"BoxBlur" | b"BoxBlurAlpha" => Self::BOX_BLUR,
            b"AdjustGamma" => Self::ADJUST_GAMMA,
            b"VideoColor" => Self::VIDEO_COLOR,
            b"LayerMovieAlpha" => Self::LAYER_MOVIE_ALPHA,
            b"LayerExTextPaint" => Self::LAYER_EX_TEXT_PAINT,
            b"LayerExRaster" => Self::LAYER_EX_RASTER,
            b"LayerExLight" => Self::LAYER_EX_LIGHT,
            b"LayerExColorize" => Self::LAYER_EX_COLORIZE,
            b"LayerExModulate" => Self::LAYER_EX_MODULATE,
            b"LayerExNoise" => Self::LAYER_EX_NOISE,
            b"LayerExWhiteNoise" => Self::LAYER_EX_WHITE_NOISE,
            b"LayerExGaussianH" => Self::LAYER_EX_GAUSSIAN_H,
            b"LayerExGaussianV" => Self::LAYER_EX_GAUSSIAN_V,
            b"ExtransWave" => Self::EXTRANS_WAVE,
            b"ExtransMosaic" => Self::EXTRANS_MOSAIC,
            b"ExtransBlurFade" => Self::EXTRANS_BLUR_FADE,
            b"ExtransTurn" => Self::EXTRANS_TURN,
            b"ExtransRotate" => Self::EXTRANS_ROTATE,
            b"ExtransRipple" => Self::EXTRANS_RIPPLE,
            b"ExtNagano" => Self::EXT_NAGANO,
            base => {
                let (photoshop, base) = match base {
                    [b'P', b's', base @ ..] => (true, base),
                    base => (false, base),
                };
                match base {
                    b"AlphaBlend" | b"AddBlend" | b"SubBlend" | b"MulBlend" | b"ScreenBlend"
                    | b"OverlayBlend" | b"HardLightBlend" | b"SoftLightBlend"
                    | b"ColorDodgeBlend" | b"ColorDodge5Blend" | b"ColorBurnBlend"
                    | b"LightenBlend" | b"DarkenBlend" | b"DiffBlend" | b"Diff5Blend"
                    | b"ExclusionBlend" => {
                        if photoshop {
                            Self::PS_BLEND
                        } else {
                            Self::NON_PS_BLEND
                        }
                    }
                    _ => Self::COPY,
                }
            }
        }
    }
}

impl From<Operation> for u32 {
    fn from(op: Operation) -> Self {
        op.0
    }
}
