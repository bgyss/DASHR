#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;
	float3 TexCoord : TEXCOORD0;
};

VS_OUTPUT vs_main( VS_INPUT In )
{
	VS_OUTPUT Out;

	// The "position" output is the coordinates in surface space,
	// So here we remap from [0,1] to [-1,+1]
	Out.Pos.x = (In.Pos.x - 0.5f) * 2.0f;
	Out.Pos.y = (0.5f - In.Pos.y) * 2.0f;
	Out.Pos.z = 1.0f;
	Out.Pos.w = 1.0f;

	Out.TexCoord = In.TexCoord;

	return Out;
}

Texture2D<float4> texEdgefill : register(t0);
Texture2D<float4> texSurfaceFromObject0 : register(t1);
Texture2D<float4> texSurfaceFromObject1 : register(t2);
Texture2D<float4> texSurfaceFromObject2 : register(t3);
SamplerState smp : register (s0);
SamplerState smpPoint : register (s1);

struct PS_OUTPUT
{
	float4 Rt0 : COLOR0;
	float4 Rt1 : COLOR1;
	float4 Rt2 : COLOR2;
};

PS_OUTPUT ps_main( VS_OUTPUT In ) : SV_TARGET
{
	PS_OUTPUT result;

	float2 edgefillSrc = texEdgefill.SampleLevel ( smpPoint, In.TexCoord, 0.0f ).xy;
	result.Rt0 = texSurfaceFromObject0.SampleLevel ( smpPoint, edgefillSrc, 0.0f );
	result.Rt1 = texSurfaceFromObject1.SampleLevel ( smpPoint, edgefillSrc, 0.0f );
	result.Rt2 = texSurfaceFromObject2.SampleLevel ( smpPoint, edgefillSrc, 0.0f );

	return result;
}



